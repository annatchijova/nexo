# NEXO — guion del video por escenas (todo grabado en vivo)

Duración objetivo: ~3:00. 4 tramos, todos grabados en vivo desde la PC
(pantalla, sin mic) + audio grabado aparte en el iPhone. Las 6 capturas
que sacaste ya NO se usan para armar el video — quedaron insertadas
directamente en `README.md` y `README_ES.md`.

Convención: **ES** = lo que decís en voz alta (Voice Memos, iPhone).
**EN** = subtítulo en inglés para quemar/subir después. Grabá cada tramo
por separado (más fácil que una toma continua).

---

### Tramo 1 — Página (0:00–0:30)
**Qué mostrás:** `nexo-web-sigma.vercel.app`, scrolleando desde arriba —
el título, los dos casos, los 3 pasos, las tarjetas de honestidad.

**ES:** "Cuando a alguien le pasa algo grave online —difusión de
contenido íntimo, un problema con sus datos— lo difícil no es solo
entender qué pasó. Es ordenarlo para que alguien más lo tome en serio.
NEXO es un espacio privado para eso: juntás mensajes, correos, PDFs y tu
propio relato, y NEXO los mantiene separados y claros."

**EN:** "When something serious happens to you online, the hard part
isn't understanding what happened — it's making it something someone
else takes seriously. NEXO is a private space for exactly that: you
gather messages, emails, PDFs, and your own account, and NEXO keeps them
separate and clear."

```bash
ffmpeg -f x11grab -framerate 30 -video_size 1920x1080 -i :0.0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p -crf 18 ~/nexo-video/video/scene1-pagina.mp4
```

---

### Tramo 2 — Demo, los dos casos (0:30–1:40)
**Qué mostrás:** `nexo-web-sigma.vercel.app/demo` → Caso 1 (acceso a
datos personales): agregar la evidencia de ejemplo, evaluar. Después
volvés y elegís Caso 2 (violencia digital): agregar el mensaje/PDF,
identificar la URL, evaluar.

**ES (Caso 1):** "Primer caso: acceso a datos personales. Agrego el
correo donde pido saber qué información tiene la empresa sobre mí,
evalúo, y NEXO me muestra si hay un camino respaldado por la Ley 25.326 —
y por qué."

**EN:** "First case: personal-data access. I add the email requesting
what information a company holds about me, evaluate, and NEXO shows
whether there's a path backed by Ley 25.326 — and why."

**ES (Caso 2):** "Segundo caso: violencia digital. Agrego el mensaje y el
PDF que identifican dónde se publicó contenido sin consentimiento,
evalúo, y NEXO cita la Ley 27.736 detrás del resultado."

**EN:** "Second case: digital violence. I add the message and PDF
identifying where content was published without consent, evaluate, and
NEXO cites Ley 27.736 behind the result."

```bash
ffmpeg -f x11grab -framerate 30 -video_size 1920x1080 -i :0.0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p -crf 18 ~/nexo-video/video/scene2-demo.mp4
```

---

### Tramo 3 — Slides (1:40–2:20)
Estas son tus propias slides (Google Slides, Canva, lo que uses) — grabás
la pantalla mientras las pasás. Sugerencia de contenido, 2-3 slides,
si todavía no las armaste:

- **Slide A:** "Tres pasos: Juntar, Entender, Preparar." + "Si la
  evidencia no alcanza, NEXO no inventa una certeza — dice qué falta."
- **Slide B:** "Cada archivo recibe una huella SHA-256." + "Un abogado,
  una plataforma o un juez puede verificarlo sin confiar en nuestra
  palabra."

**ES:** "Todo pasa por tres pasos: juntar, entender, preparar. Y cada
archivo que subís recibe una huella SHA-256, así cualquiera puede
verificar que no se alteró, sin tener que confiar en nuestra palabra."

**EN:** "It's three steps: gather, understand, prepare. And every file
you upload gets a SHA-256 fingerprint, so anyone can verify it wasn't
altered, without just taking our word for it."

```bash
ffmpeg -f x11grab -framerate 30 -video_size 1920x1080 -i :0.0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p -crf 18 ~/nexo-video/video/scene3-slides.mp4
```

---

### Tramo 4 — GitHub (2:20–2:50)
**Qué mostrás:** `github.com/annatchijova/nexo` → carpeta `crates/`
(los bundles legales, los extractores) y, si da el tiempo, un contrato en
`docs/`.

**ES:** "Atrás de esto hay un backend en Rust, con extractores aislados
en sandbox, tests corridos contra una base de datos real, y estas dos
leyes argentinas ya conectadas, no simuladas."

**EN:** "Behind this there's a Rust backend, with sandboxed extractors,
tests run against a real database, and these two Argentine laws actually
wired in, not simulated."

```bash
ffmpeg -f x11grab -framerate 30 -video_size 1920x1080 -i :0.0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p -crf 18 ~/nexo-video/video/scene4-github.mp4
```

---

### Cierre (2:50–3:00)
Podés decirlo hablando a cámara en el último segundo del Tramo 4, sin
necesidad de un tramo aparte:

**ES:** "NEXO no reemplaza a un abogado. Hace posible el primer paso:
preservar, entender y prepararte, aunque no sepas nada de tecnología."

**EN:** "NEXO doesn't replace a lawyer. It makes the first step possible:
preserving, understanding, and preparing — even if you know nothing about
technology."

---

## Cómo seguimos

1. Grabás los 4 tramos en `~/nexo-video/video/` con los comandos de
   arriba (`Ctrl+C` para cortar cada uno).
2. Grabás el audio de cada tramo en el iPhone (Voice Memos), lo mandás
   por mail a tu propio correo, y lo bajás a `~/nexo-video/audio/` como
   `scene1.m4a`, `scene2.m4a`, `scene3.m4a`, `scene4.m4a`.
3. Avisame — junto video + audio de cada tramo con `ffmpeg`, concateno
   los 4 en orden, y con la duración real te armo el `.srt` en inglés
   para subir junto al video a YouTube.
