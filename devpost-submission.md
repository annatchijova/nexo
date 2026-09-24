# NEXO — Evidencia verificable para derechos digitales

## Estado de este documento

Este es un borrador en español para la presentación de NEXO en LexHack 2026.
Todavía no se envió nada a Devpost. La fecha oficial de cierre de submissions
es el 27 de septiembre de 2026 a las 21:00 ET.

## Título

NEXO — Evidencia verificable para derechos digitales

## Resumen de una línea

NEXO convierte mensajes, correos y documentos dispersos sobre violencia digital,
privacidad y datos personales en un caso claro, revisable y verificable, sin
reemplazar al abogado ni enviar nada por la persona.

## El problema

Cuando una persona sufre una publicación no autorizada, violencia digital o un
problema con sus datos personales, la información suele quedar repartida entre
capturas, conversaciones, correos, PDFs y recuerdos. El problema no es solo
entender qué ocurrió: es poder explicar qué se conserva, qué se sabe, qué falta
y qué puede revisarse después.

Las herramientas que prometen “conocer tus derechos” suelen mezclar el relato
de la persona con la evidencia y con una conclusión automática. Eso puede ser
confuso y peligroso, especialmente para alguien que está asustado, no conoce
la tecnología forense y necesita preparar material para hablar con un abogado,
una organización de apoyo o una autoridad.

## La solución, explicada para una persona

NEXO ofrece un espacio privado para ordenar lo que pasó.

La persona puede agregar un mensaje, un correo `.eml`, un PDF, texto y su
propio relato. NEXO conserva cada elemento por separado y explica:

- qué proviene directamente de un archivo;
- qué fue afirmado por la persona;
- qué conclusión o posibilidad surge del conjunto;
- qué requisito todavía falta;
- por qué aparece una determinada vía posible;
- qué material puede prepararse para que la persona lo revise.

La interfaz está pensada para alguien que no es abogado ni especialista
forense. La DEMO permite probar dos casos completos sin token ni backend real:

1. un pedido de acceso a datos personales;
2. una situación de violencia digital o publicación no autorizada.

La DEMO genera un `.eml`, un PDF y un manifiesto JSON con las huellas SHA-256
de los archivos descargados. El espacio conectado agrega evidencia real,
evalúa el caso y prepara exportaciones desde el backend.

NEXO no dice que un archivo “prueba la verdad” por sí mismo. Muestra qué bytes
se conservaron y qué afirmaciones están respaldadas. Tampoco presenta trámites,
contacta plataformas ni actúa legalmente por la persona.

## Por qué importa

Una persona no debería necesitar conocer RFC 5322, hashes, extractores o
serialización canónica para empezar a conservar información importante.

NEXO traduce una tarea técnica —preservar evidencia y su procedencia— en un
flujo comprensible: reunir, entender, revisar y preparar.

También ayuda a que un abogado, una organización o una autoridad pueda
distinguir entre el archivo original, lo que la persona declara y lo que el
sistema infiere. Esa separación reduce la falsa certeza y hace que las
limitaciones queden visibles.

## Casos de uso

- Acceso, rectificación o supresión de datos personales.
- Violencia digital.
- Publicación o difusión no autorizada de contenido íntimo.
- Preparación de evidencia para revisión legal.
- Organización de correos, PDFs y relatos antes de consultar a un profesional.
- Exportación de material portable y verificable.

## Qué funciona hoy

- DEMO pública sin token: https://nexo-web-sigma.vercel.app/demo
- Página principal: https://nexo-web-sigma.vercel.app
- Repositorio público: https://github.com/annatchijova/nexo
- Ingesta de texto plano, `.eml` y PDF.
- Extractores aislados y acotados para evitar que un archivo hostil bloquee el
  servidor.
- Grafo de caso que separa artefactos, observaciones, afirmaciones e inferencias.
- Bundle argentino de Ley 25.326 para datos personales.
- Bundle argentino de Ley 27.736 para violencia digital.
- Evaluaciones con resultado positivo, condicional o negativo honesto.
- Preparación de pedidos y paquetes de evidencia sin envío automático.
- SHA-256 para artefactos, exportaciones y bundles legales.
- Manifiestos y verificación independiente de exportaciones del backend.

## Cómo se usa la DEMO

1. Abrir https://nexo-web-sigma.vercel.app o https://nexo-web-sigma.vercel.app/demo.
2. Elegir “Acceso a datos personales” o “Violencia digital”.
3. Agregar la evidencia de ejemplo.
4. Completar el requisito del caso.
5. Evaluar el caso.
6. Preparar los materiales.
7. Descargar el `.eml`, el PDF y el manifiesto JSON con SHA-256.

La DEMO no crea un caso real ni envía información. Es una forma de entender
el recorrido antes de conectar un espacio privado.

## Cómo se usa el espacio real

El token no se inventa en la página. Lo entrega la persona que administra el
servidor NEXO junto con la dirección HTTPS de la API. En el servidor, la
administración puede recuperar la credencial inicial con:

```bash
sudo awk -F= '$1=="NEXO_BOOTSTRAP_OWNER"{print $2}' /etc/nexo/nexo-api.env
```

Ese valor se entrega por un canal privado y nunca se publica en el repositorio,
una captura, un issue o el código fuente.

## Cómo se usa la inteligencia artificial

NEXO no usa un modelo generativo para decidir si una persona tiene razón, para
inventar una norma ni para enviar una acción legal.

La evaluación central es determinista y está basada en bundles legales
versionados, fuentes capturadas, requisitos explícitos y evidencia registrada.
Si la evidencia no alcanza, el sistema devuelve una salida negativa o
condicional con lo que falta.

Durante el desarrollo se utilizaron herramientas de asistencia de IA,
incluyendo Codex y Claude, para explorar, implementar, revisar y documentar
partes del proyecto. Ese uso se declara porque las reglas permiten herramientas
de generación de código, pero el equipo debe poder explicar y revisar el código
final. La lógica crítica, los contratos, las pruebas y las decisiones de
seguridad están en el repositorio y no dependen de una respuesta oculta de un
modelo.

## Cómo se usó Codex en el proceso

Codex ayudó a:

- revisar la arquitectura existente antes de modificarla;
- implementar y revisar la interfaz bilingüe, la DEMO y los estados de error;
- investigar el problema de autenticación `401: expected Bearer token`;
- conectar la explicación del token con el flujo de usuario;
- agregar la generación de materiales de DEMO y sus huellas SHA-256;
- revisar contraste, navegación y lenguaje para personas no técnicas;
- ejecutar compilaciones, pruebas y verificaciones del deployment;
- preparar esta documentación sin ocultar limitaciones.

Claude también participó en la preparación de casos de ejemplo. Las
contribuciones de herramientas de IA se revisaron contra el código, los
contratos y las pruebas del repositorio.

## Características principales

### Para una persona que necesita ayuda

- Interfaz bilingüe español/inglés.
- DEMO visible desde la portada y desde el espacio local.
- Explicación explícita de dónde sale el token.
- Instrucciones simples para exportar un correo `.eml`.
- Alternativa PDF cuando el proveedor de correo no permite descargar el mail.
- Explicación de SHA-256 en lenguaje común.
- Casos de uso visibles: datos personales, violencia digital, publicación no
  autorizada y preparación de evidencia.
- Mensajes de error que explican qué hacer ante un `401`.

### Para una revisión técnica o legal

- Artefactos originales conservados con digest SHA-256.
- Observaciones vinculadas al artefacto y a un locator concreto.
- Declaraciones de la persona modeladas como una categoría distinta.
- Fuentes normativas capturadas y citadas.
- Evaluaciones con resultado explicable.
- Preparaciones y exportaciones deterministas.
- Verificador independiente sin necesidad del servidor o la base de datos.
- Límites de tamaño y procesamiento para entradas hostiles.

## Arquitectura técnica

NEXO está dividido en capas:

- `nexo-core`: modelo de dominio y tipos del grafo.
- `nexo-policy-ar` y `nexo-policy-ar-digital-violence`: bundles legales
  argentinos versionados.
- `nexo-app`: transacciones, persistencia y autorización.
- `nexo-api`: única superficie HTTP expuesta.
- `nexo-sandbox`: ejecución aislada de extractores.
- `nexo-extractor-plaintext`, `nexo-extractor-eml` y `nexo-extractor-pdf`:
  extracción acotada por formato.
- `nexo-integrity`: SHA-256, serialización canónica y cadena de auditoría.
- `nexo-verifier`: verificación independiente.
- `nexo-report`: informes Markdown, HTML y PDF.
- `web`: cliente TypeScript/Vite bilingüe.

La ruta crítica es:

```text
archivo o texto
  -> registro del artefacto y SHA-256
  -> extracción aislada y acotada
  -> observaciones vinculadas
  -> evaluación contra bundle legal
  -> resultado explicado
  -> preparación y exportación verificable
```

## Tech stack y créditos

- Rust.
- Axum para la API HTTP.
- PostgreSQL.
- TypeScript, Vite y CSS para la web.
- Docker para el aislamiento de extractores.
- `printpdf` y herramientas de renderización de reportes.
- SHA-256 y serialización canónica para integridad.
- Ley 25.326 y Ley 27.736 como fuentes legales argentinas capturadas.
- Codex y Claude como herramientas de asistencia durante el desarrollo,
  declaradas y revisadas por el equipo.

## Pruebas

Para construir el frontend:

```bash
cd web
npm ci
npm run build
```

Para la suite Rust:

```bash
cargo test --workspace
```

Pruebas adicionales del repositorio:

```bash
./scripts/test_schema.sh
./scripts/test_repository.sh
./scripts/test_api.sh
```

Las pruebas cubren el grafo de evidencia, límites de extractores, `.eml`, PDF,
persistencia, evaluación, preparación, exportación, hashes, autorización y
rechazos controlados.

## Demo video — guion de 2 a 3 minutos

### 0:00–0:25 — El problema humano

“Cuando alguien sufre violencia digital o un problema con sus datos, la
información queda repartida entre mails, PDFs, mensajes y capturas. NEXO ayuda
a convertir eso en un registro claro sin prometer una certeza legal falsa.”

### 0:25–0:55 — Entrada simple

Mostrar la portada, los casos de uso y la DEMO grande. Explicar que no requiere
token y que está pensada para una persona no técnica.

### 0:55–1:35 — Caso de ejemplo

Elegir violencia digital, agregar el mensaje/PDF, identificar la URL, evaluar y
mostrar el resultado respaldado.

### 1:35–2:05 — Verificabilidad

Preparar materiales y descargar el `.eml`, PDF y manifiesto. Mostrar el SHA-256
y explicar que prueba identidad de bytes, no la verdad completa del relato.

### 2:05–2:35 — Backend y límites

Mostrar brevemente el repositorio, los extractores aislados, el bundle legal y
una exportación real del backend. Aclarar que NEXO prepara, pero no envía ni
presenta acciones.

### 2:35–3:00 — Cierre

“NEXO no reemplaza a un abogado. Hace que el primer paso —preservar,
entender y preparar— sea posible para alguien que no conoce la tecnología.”

## Lista de capturas

- [ ] Portada con la DEMO grande y los dos casos.
- [ ] Caso de datos personales antes de agregar evidencia.
- [ ] Caso de violencia digital con PDF y ubicación identificada.
- [ ] Resultado respaldado y explicación de por qué aparece.
- [ ] Descargas `.eml`, PDF y manifiesto SHA-256.
- [ ] Espacio real con token oculto y carga de evidencia.
- [ ] Exportación real del backend y digest verificable.

## Enlaces públicos

- Repositorio: `https://github.com/annatchijova/nexo`
- Demo: `https://nexo-web-sigma.vercel.app`
- Demo directa: `https://nexo-web-sigma.vercel.app/demo`
- Video: **PENDIENTE — agregar enlace de YouTube, Vimeo o Loom**

## Limitaciones conocidas

- La jurisdicción implementada actualmente es Argentina.
- No hay OCR para capturas o PDFs escaneados sin capa de texto.
- La DEMO es local al navegador y no representa un caso real del backend.
- El frontend público todavía requiere configurar y operar un backend para
  crear casos reales.
- No debe presentarse NEXO como abogado, asesoramiento legal o servicio de
  emergencias.
- La huella SHA-256 demuestra identidad de bytes, no autoría, autenticidad
  semántica ni veracidad jurídica por sí sola.

## Criterios del hackathon y estrategia

### Impacto y viabilidad — 25%

Presentar el problema humano primero: personas no técnicas necesitan ordenar
material sobre derechos digitales y llegar mejor preparadas a una revisión
legal. Mostrar que el flujo ya funciona y que no automatiza una acción externa.

### Ejecución técnica — 25%

Mostrar el backend Rust, PostgreSQL, extractores aislados, bundles legales,
tests, evaluación y exportación verificable. No limitar el video a la DEMO de
frontend.

### Experiencia y diseño — 20%

Mostrar la interfaz bilingüe, el lenguaje para no especialistas, la DEMO sin
token, los mensajes de error, la explicación `.eml`/PDF y el contraste claro.

### Innovación — 15%

Enfatizar la combinación de evidencia separada, evaluación legal acotada y
verificación independiente. Evitar describirlo como “un chatbot legal”.

### Presentación y documentación — 15%

Completar el video, capturas, README, tech stack, créditos de IA y limitaciones.

## TODO oficial antes de enviar

- [ ] Confirmar nombre y resumen final.
- [ ] Grabar y subir el video de máximo 3 minutos.
- [ ] Tomar 3–7 capturas de la aplicación funcionando.
- [ ] Verificar el flujo real del backend con una API pública operativa.
- [ ] Confirmar que todos los integrantes estén listados en Devpost.
- [ ] Declarar Codex, Claude, librerías, APIs y herramientas utilizadas.
- [ ] Revisar que no haya tokens, claves, `.env` ni credenciales en el repo.
- [ ] Completar el enlace del video.
- [ ] Revisar la elegibilidad: el evento está dirigido a estudiantes y permite
  equipos de hasta cuatro personas.
- [ ] Enviar el proyecto desde Devpost antes del cierre oficial.
