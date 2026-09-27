# NEXO — guion del video por escenas (para grabar + subtitular)

Duración total objetivo: ~3:00. 8 escenas. Grabá cada escena por separado
(audio y, si corresponde, pantalla) — es mucho más fácil que una toma
continua, y después las unís en orden.

Convención: **Visual** = qué mostrar (una de tus 6 capturas de `visual/`,
o pantalla en vivo). **ES** = lo que decís en voz alta. **EN** = texto del
subtítulo quemado (traducilo tal cual, no hace falta que sea literal
palabra por palabra, ya está pensado para leerse rápido).

---

### Escena 1 — 0:00–0:20
**Visual:** `Screenshot from 2026-09-27 12-11-17.png` (el hero "Understand what happened...")

**ES:** "Cuando a alguien le pasa algo grave online —difusión de
contenido íntimo, un problema con sus datos— lo difícil no es solo
entender qué pasó. Es ordenarlo para que alguien más lo tome en serio."

**EN (subtítulo):** "When something serious happens to you online, the
hard part isn't understanding what happened — it's making it something
someone else takes seriously."

---

### Escena 2 — 0:20–0:40
**Visual:** `Screenshot from 2026-09-27 12-12-08.png` (private legal preparation workspace)

**ES:** "NEXO es un espacio privado para eso: juntás mensajes, correos,
PDFs y tu propio relato, y NEXO los mantiene separados y claros."

**EN:** "NEXO is a private space for exactly that: you gather messages,
emails, PDFs, and your own account — and NEXO keeps them separate and
clear."

---

### Escena 3 — 0:40–1:15 (LA MÁS LARGA — acá va la demo en vivo)
**Visual:** pantalla en vivo, grabada con OBS — abrís `nexo-web-sigma.vercel.app/demo`,
elegís el caso de violencia digital, agregás la evidencia de ejemplo,
identificás la URL, evaluás.

**ES (narrás mientras clickeás, no hace falta que sea guion palabra por
palabra, es lo más natural del video):** "Así se ve. Elijo un caso de
violencia digital, agrego la evidencia, identifico dónde se publicó, y
evalúo. NEXO me dice si hay un camino respaldado por ley — y por qué."

**EN:** "Here it is. I pick a digital-violence case, add the evidence,
identify where it was published, and evaluate. NEXO tells me if there's
a path backed by law — and why."

---

### Escena 4 — 1:15–1:35
**Visual:** `Screenshot from 2026-09-27 12-12-46.png` (panel de caso real)

**ES:** "Esto no es solo una demo de mentira: es el mismo flujo que corre
en un espacio real, conectado a un backend de verdad."

**EN:** "This isn't just a mock demo — it's the same flow running in a
real, connected workspace."

---

### Escena 5 — 1:35–2:00
**Visual:** `Screenshot from 2026-09-27 12-11-46.png` (3 pasos + tarjetas de honestidad)

**ES:** "Todo pasa por tres pasos: juntar, entender, preparar. Y si la
evidencia no alcanza, NEXO no inventa una certeza — dice exactamente qué
falta."

**EN:** "It's three steps: gather, understand, prepare. And if the
evidence isn't enough, NEXO doesn't fake certainty — it says exactly what
is missing."

---

### Escena 6 — 2:00–2:25
**Visual:** `Screenshot from 2026-09-27 12-12-27.png` (SHA-256 / token / verificabilidad)

**ES:** "Cada archivo que subís recibe una huella SHA-256, así cualquiera
—un abogado, una plataforma, un juez— puede verificar que no se alteró,
sin tener que confiar en nuestra palabra."

**EN:** "Every file you upload gets a SHA-256 fingerprint, so anyone — a
lawyer, a platform, a judge — can verify it wasn't altered, without just
taking our word for it."

---

### Escena 7 — 2:25–2:45
**Visual:** pantalla en vivo brevemente sobre el repo de GitHub (o, si no
da el tiempo, reusá `Screenshot from 2026-09-27 12-10-59.png`)

**ES:** "Por atrás hay un backend en Rust, con extractores aislados y dos
leyes argentinas reales conectadas: la 25.326 y la 27.736."

**EN:** "Behind it there's a Rust backend, with isolated extractors and
two real Argentine laws wired in: Ley 25.326 and Ley 27.736."

---

### Escena 8 — 2:45–3:00 (cierre)
**Visual:** `Screenshot from 2026-09-27 12-10-59.png` (portada, con el logo) o el logo solo

**ES:** "NEXO no reemplaza a un abogado. Hace posible el primer paso:
preservar, entender y prepararte, aunque no sepas nada de tecnología."

**EN:** "NEXO doesn't replace a lawyer. It makes the first step possible:
preserving, understanding, and preparing — even if you know nothing about
technology."

---

## Nota sobre los subtítulos

Si el editor que uses no permite quemar subtítulos fácil, una alternativa
rápida: generá un `.srt` con los textos EN de arriba (con timestamps
aproximados de cada escena) y subilo junto al video a YouTube — YouTube
puede quemarlos o mostrarlos como closed captions, cualquiera de las dos
formas cumple con "que se entienda en inglés". Si querés, te armo el
`.srt` ya con timestamps una vez que sepas cuánto dura cada escena
grabada de verdad (los tiempos de arriba son estimados).
