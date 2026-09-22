# NEXO en AWS

Esta guía prepara una instancia EC2 para una instalación personal de NEXO.
La instancia ejecuta el API como servicio del host porque el extractor
sandbox necesita invocar Docker con límites explícitos. PostgreSQL y los
objetos se mantienen en almacenamiento persistente bajo control de la
instancia.

## Variables requeridas

Configurar `/etc/nexo/nexo-api.env` con permisos `0600`:

```text
DATABASE_URL=postgres://nexo:<password>@127.0.0.1:5432/nexo
NEXO_BOOTSTRAP_OWNER=<token-largo-generado-fuera-del-repositorio>
NEXO_OBJECT_STORE_ROOT=/var/lib/nexo/objects
NEXO_EXPORT_ROOT=/var/lib/nexo/exports
NEXO_BIND_ADDR=127.0.0.1:8080
NEXO_WEB_ORIGIN=https://nexo-web-sigma.vercel.app
```

El token de `NEXO_BOOTSTRAP_OWNER` nunca debe aparecer en logs, comandos
guardados, Git ni documentación pública.

## Orden de instalación

1. Amazon Linux 2023, EBS persistente y acceso administrativo por SSH o SSM.
2. Docker, PostgreSQL y las dependencias de compilación.
3. Crear la base `nexo`, aplicar `crates/nexo-app/migrations/0001_init.sql` y
   construir la imagen `nexo-extractor-plaintext:local` con
   `scripts/build_extractors.sh`.
4. Instalar el binario `nexo-api` en `/opt/nexo/nexo-api`.
5. Instalar `deploy/aws/nexo-api.service`, habilitarlo y arrancarlo.
6. Colocar Caddy o un balanceador TLS delante de `127.0.0.1:8080`.
7. Configurar `VITE_NEXO_API_BASE` en Vercel con la URL HTTPS del API.

Antes de usar evidencia real, verificar `/healthz`, aplicar la migración en
una base nueva y probar creación de caso, ingestión, evaluación, exportación
y descarga desde la UI.
