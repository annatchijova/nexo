# NEXO en AWS

Prepara una instancia EC2 para una instalación personal de NEXO. La
instancia ejecuta el API como servicio del host porque el extractor
sandbox necesita invocar Docker con límites explícitos. PostgreSQL y los
objetos se mantienen en almacenamiento persistente bajo control de la
instancia.

Esta guía y `provision.sh` cubren todo lo que no toca la cuenta de AWS en
sí (paquetes, base de datos, binario, servicio). Crear la instancia EC2, el
security group y la IP elástica es un paso aparte, deliberadamente fuera de
este script — son recursos facturados de la cuenta y esa decisión le
corresponde a quien los paga, no a un script.

## 0. Crear la instancia (fuera de este repo)

Amazon Linux 2023, un `t3.small` alcanza para empezar (2 vCPU, 2 GiB —
subir si el volumen de evidencia lo justifica), EBS persistente (20 GiB
mínimo), y un security group que permita:

- 22/tcp (SSH) solo desde tu IP.
- 443/tcp (HTTPS) desde donde sea — es lo único público.
- 80/tcp (HTTP) desde donde sea — solo para el desafío ACME de Let's Encrypt.

## 1. Provisionar el host

Con la instancia corriendo y acceso SSH:

```sh
git clone https://github.com/annatchijova/nexo.git
cd nexo
sudo ./deploy/aws/provision.sh
```

`provision.sh` es idempotente — instala Docker y PostgreSQL 16, crea el
usuario de sistema `nexo`, el rol y la base `nexo` en Postgres, compila los
tres extractores sandboxeados (`scripts/build_extractors.sh`) y el binario
`nexo-api` en release, escribe `/etc/nexo/nexo-api.env` (solo si no existe
— nunca pisa secretos ya generados), e instala y arranca
`deploy/aws/nexo-api.service`.

La primera corrida imprime el `NEXO_BOOTSTRAP_OWNER` recién generado — es
el bearer token del único dueño de la instancia. Guardalo en un lugar
seguro ahora; no se vuelve a imprimir, y sin él no hay forma de autenticar
contra la API.

## 2. Variables en `/etc/nexo/nexo-api.env` (permisos `0600`)

`provision.sh` ya las escribe; esta es la referencia de qué hace cada una:

```text
DATABASE_URL=postgres://nexo:<password>@127.0.0.1:5432/nexo
NEXO_BOOTSTRAP_OWNER=<token largo, generado por provision.sh>
NEXO_OBJECT_STORE_ROOT=/var/lib/nexo/objects
NEXO_EXPORT_ROOT=/var/lib/nexo/exports
NEXO_BIND_ADDR=127.0.0.1:8080
NEXO_WEB_ORIGIN=https://nexo-web-sigma.vercel.app
NEXO_AUDIT_HMAC_KEY=<clave larga, generada por provision.sh>
NEXO_AUDIT_HMAC_KEY_VERSION=v1
```

`NEXO_AUDIT_HMAC_KEY` y `NEXO_AUDIT_HMAC_KEY_VERSION` son obligatorias, no
opcionales: `nexo_app::audit::append` (el camino que registra cada
mutación — evidencia, aserciones, evaluaciones, preparaciones,
credenciales) las requiere y falla sin ellas, así que **sin estas dos
variables el API rechaza toda escritura**, no solo una función de auditoría
específica. `NEXO_AUDIT_HMAC_KEY_VERSION` puede quedar como `v1`
indefinidamente en una instancia de un solo operador; solo hace falta
incrementarla si alguna vez rotás la clave.

Ninguno de estos valores debe aparecer en logs, comandos guardados, Git ni
documentación pública.

## 3. TLS

`nexo-api` escucha solo en `127.0.0.1:8080` — nada expone eso a la red
directamente. `deploy/aws/Caddyfile` pone Caddy delante como terminador TLS,
con renovación automática de Let's Encrypt (sin paso manual de certbot).

El repositorio COPR `@caddy/caddy` **no publica build para Amazon Linux
2023** (solo Fedora/EPEL/openSUSE) — confirmado al intentarlo en un deploy
real, no asumido. Instalar el binario oficial directamente es el camino que
sí funciona:

```sh
CADDY_VERSION=$(curl -s https://api.github.com/repos/caddyserver/caddy/releases/latest | grep -o '"tag_name": *"[^"]*"' | cut -d'"' -f4)
curl -sL -o /tmp/caddy.tar.gz "https://github.com/caddyserver/caddy/releases/download/${CADDY_VERSION}/caddy_${CADDY_VERSION#v}_linux_amd64.tar.gz"
tar xzf /tmp/caddy.tar.gz -C /tmp caddy
sudo install -m 0755 /tmp/caddy /usr/local/bin/caddy

sudo mkdir -p /etc/caddy /var/log/caddy /var/lib/caddy
sudo id caddy >/dev/null 2>&1 || sudo useradd --system --home /var/lib/caddy --shell /sbin/nologin caddy
sudo chown -R caddy:caddy /var/lib/caddy /var/log/caddy

# editar deploy/aws/Caddyfile: reemplazar api.example.com por tu dominio real
# (o, sin dominio propio, "<ip-con-guiones>.sslip.io" — ver nota abajo)
sudo install -m 0644 deploy/aws/Caddyfile /etc/caddy/Caddyfile
sudo install -m 0644 deploy/aws/caddy.service /etc/systemd/system/caddy.service
sudo systemctl daemon-reload
sudo systemctl enable --now caddy
```

Necesitás un nombre DNS real apuntando a la IP de la instancia antes de este
paso — Caddy no puede emitir un certificado válido para una IP desnuda. Si
no tenés un dominio propio todavía, [sslip.io](https://sslip.io) resuelve
`<ip-con-guiones>.sslip.io` a esa IP automáticamente sin registro previo
(por ejemplo, la IP `3.150.146.250` es `3-150-146-250.sslip.io`) — es DNS
público real, así que Let's Encrypt lo valida igual que un dominio propio.
Cuando consigas un dominio propio, es cambiar esa línea en el Caddyfile y
recargar (`sudo systemctl reload caddy`).

## 4. Conectar el frontend

Configurar `VITE_NEXO_API_BASE` en Vercel con la URL HTTPS del paso
anterior (`https://tu-dominio`, o `https://<ip-con-guiones>.sslip.io`).

## 5. Verificar antes de usar evidencia real

```sh
curl -s https://tu-dominio/healthz
```

Después, desde la UI (o con `curl`, ver `docs/API_CONTRACT.md`): crear un
caso, agregar evidencia de los tres tipos (`plain_text`, `eml`, `pdf`),
evaluar, preparar (`draft_request` y `evidence_package`), exportar, y
verificar el export con `nexo-verify` de forma independiente. Esta
secuencia completa, incluyendo el tramo por HTTPS público a través de
Caddy (no solo contra `127.0.0.1`), fue corrida contra una instancia EC2
real (Amazon Linux 2023, `t3.small`) provisionada con este mismo
`provision.sh` — ver el "Status" del
[readme técnico](../../docs/TECHNICAL_README.md) para el detalle exacto de
qué se verificó, cuándo, y los tres bugs de deploy reales que esa corrida
encontró y que `provision.sh` ya tiene arreglados (target musl faltante,
swap insuficiente en `t3.small`, autenticación `ident` de Postgres).

## Backups

No automatizados todavía por `provision.sh`. Como mínimo: `pg_dump`
periódico de la base `nexo` y una copia de `/var/lib/nexo/objects` y
`/var/lib/nexo/exports` (contienen los bytes originales de la evidencia —
perderlos sin aviso no es aceptable para lo que este proyecto se propone
guardar). Snapshots de EBS son la opción más simple si el volumen completo
está bajo `/var/lib/nexo`.
