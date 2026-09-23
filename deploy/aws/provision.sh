#!/usr/bin/env bash
# Provisions a single-tenant NEXO instance on a fresh Amazon Linux 2023 EC2
# host: system packages, PostgreSQL role/database, the three sandboxed
# extractor images, the nexo-api release binary, and the systemd service.
# Idempotent — safe to re-run after a package update or a repo pull; it
# never overwrites an already-generated secret in /etc/nexo/nexo-api.env.
#
# Does NOT provision the EC2 instance, security group, or elastic IP
# itself (that's an aws-cli/console/Terraform step outside this script,
# since it touches billed account resources this script has no business
# deciding about) and does NOT set up TLS (see deploy/aws/Caddyfile,
# installed separately once NEXO_DOMAIN is known).
#
# Usage: run as root (or via sudo) on the target instance:
#   NEXO_REPO_URL=https://github.com/annatchijova/nexo.git \
#   NEXO_REPO_REF=main \
#   ./provision.sh
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "run as root (sudo ./provision.sh)" >&2
  exit 1
fi

NEXO_REPO_URL="${NEXO_REPO_URL:-https://github.com/annatchijova/nexo.git}"
NEXO_REPO_REF="${NEXO_REPO_REF:-main}"
NEXO_CHECKOUT="/opt/nexo/src"
NEXO_HOME="/var/lib/nexo"
NEXO_ENV_FILE="/etc/nexo/nexo-api.env"
PG_DB="nexo"
PG_ROLE="nexo"

echo "== swap (a 2 GiB-class instance like t3.small OOM-kills rustc partway"
echo "   through the workspace build otherwise -- confirmed by actually"
echo "   provisioning on one; nexo-report alone pulls in a PDF-rendering"
echo "   stack heavy enough to need this) =="
if [ ! -f /swapfile ]; then
  fallocate -l 4G /swapfile
  chmod 600 /swapfile
  mkswap /swapfile
  swapon /swapfile
  echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

echo "== packages =="
dnf install -y docker postgresql16-server postgresql16 gcc gcc-c++ make \
  pkgconfig openssl-devel git tar

systemctl enable --now docker
if [ ! -d /var/lib/pgsql/data/base ]; then
  postgresql-setup --initdb
  # AL2023's default pg_hba.conf uses `ident` for TCP connections to
  # 127.0.0.1/::1, which rejects nexo-api's password-authenticated
  # DATABASE_URL outright ("Ident authentication failed for user
  # \"nexo\"") -- confirmed by actually provisioning and watching the
  # service fail on its very first start. scram-sha-256 is what the role
  # created below is actually given a password for.
  sed -i 's/ident$/scram-sha-256/' /var/lib/pgsql/data/pg_hba.conf
fi
systemctl enable --now postgresql

echo "== nexo system user =="
id -u nexo >/dev/null 2>&1 || useradd --system --home-dir "$NEXO_HOME" --shell /sbin/nologin nexo
usermod -aG docker nexo
mkdir -p "$NEXO_HOME/objects" "$NEXO_HOME/exports" /etc/nexo /opt/nexo
chown -R nexo:nexo "$NEXO_HOME" /opt/nexo
chmod 750 "$NEXO_HOME"

echo "== rust toolchain (as the nexo user, matching rust-toolchain.toml) =="
if ! sudo -u nexo test -x "$NEXO_HOME/.cargo/bin/cargo"; then
  sudo -u nexo bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal'
fi
CARGO="$NEXO_HOME/.cargo/bin/cargo"

echo "== source checkout =="
if [ -d "$NEXO_CHECKOUT/.git" ]; then
  sudo -u nexo git -C "$NEXO_CHECKOUT" fetch origin "$NEXO_REPO_REF"
  sudo -u nexo git -C "$NEXO_CHECKOUT" checkout "$NEXO_REPO_REF"
  sudo -u nexo git -C "$NEXO_CHECKOUT" reset --hard "origin/$NEXO_REPO_REF"
else
  sudo -u nexo git clone --branch "$NEXO_REPO_REF" "$NEXO_REPO_URL" "$NEXO_CHECKOUT"
fi

echo "== database role and database (idempotent) =="
if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='${PG_ROLE}'" | grep -q 1; then
  PG_PASSWORD="$(openssl rand -hex 24)"
  sudo -u postgres psql -c "CREATE ROLE ${PG_ROLE} LOGIN PASSWORD '${PG_PASSWORD}';"
  echo "$PG_PASSWORD" > /etc/nexo/.pg_password.new
  echo "generated a new database password — see the DATABASE_URL note below"
fi
sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='${PG_DB}'" | grep -q 1 \
  || sudo -u postgres psql -c "CREATE DATABASE ${PG_DB} OWNER ${PG_ROLE};"

echo "== sandboxed extractor images =="
sudo -u nexo bash -c "cd '$NEXO_CHECKOUT' && PATH=\"$NEXO_HOME/.cargo/bin:\$PATH\" ./scripts/build_extractors.sh"

echo "== nexo-api release binary =="
sudo -u nexo bash -c "cd '$NEXO_CHECKOUT' && '$CARGO' build --release -p nexo-api"
install -m 0755 -o nexo -g nexo "$NEXO_CHECKOUT/target/release/nexo-api" /opt/nexo/nexo-api

echo "== environment file =="
if [ ! -f "$NEXO_ENV_FILE" ]; then
  DB_PASSWORD_LINE="<see /etc/nexo/.pg_password.new if this is the first run, then delete that file>"
  if [ -f /etc/nexo/.pg_password.new ]; then
    DB_PASSWORD_LINE="$(cat /etc/nexo/.pg_password.new)"
  fi
  cat > "$NEXO_ENV_FILE" <<EOF
DATABASE_URL=postgres://${PG_ROLE}:${DB_PASSWORD_LINE}@127.0.0.1:5432/${PG_DB}
NEXO_BOOTSTRAP_OWNER=$(openssl rand -hex 32)
NEXO_OBJECT_STORE_ROOT=${NEXO_HOME}/objects
NEXO_EXPORT_ROOT=${NEXO_HOME}/exports
NEXO_BIND_ADDR=127.0.0.1:8080
NEXO_WEB_ORIGIN=https://nexo-web-sigma.vercel.app
NEXO_AUDIT_HMAC_KEY=$(openssl rand -hex 32)
NEXO_AUDIT_HMAC_KEY_VERSION=v1
EOF
  chmod 0600 "$NEXO_ENV_FILE"
  chown nexo:nexo "$NEXO_ENV_FILE"
  rm -f /etc/nexo/.pg_password.new
  echo "wrote a fresh $NEXO_ENV_FILE — NEXO_BOOTSTRAP_OWNER is the owner's"
  echo "bearer token; record it somewhere safe now, it is never printed again"
else
  echo "$NEXO_ENV_FILE already exists, left untouched"
fi

echo "== systemd service =="
install -m 0644 "$NEXO_CHECKOUT/deploy/aws/nexo-api.service" /etc/systemd/system/nexo-api.service
systemctl daemon-reload
systemctl enable --now nexo-api

echo "== done =="
sleep 2
systemctl --no-pager status nexo-api || true
echo "verify with: curl -s http://127.0.0.1:8080/healthz"
echo "next: install deploy/aws/Caddyfile with your real domain for TLS (see deploy/aws/README.md)"
