import "./styles.css";
const logoUrl = "/logo.png";

type CaseNode = {
  node_id: number;
  kind: "artifact" | "observation" | "user_assertion" | "derived_fact" | "inference";
  created_at: string;
  confirmed: boolean | null;
};

type CaseDetail = { case_id: number; nodes: CaseNode[] };
type EvaluationRecord = { evaluation_id: number; result: Evaluation };
type Preparation = {
  preparation_id: number;
  status: "prepared" | "exported";
  manifest_digest?: string;
  artifact_count?: number;
};
type Citation = { proposition: string; source_issuer: string; source_locator: string };
type Evaluation = {
  kind: "actionable" | "non_actionable";
  status?: "supported" | "conditionally_supported";
  available?: boolean;
  variant?: string;
  cause?: string;
  factual_support?: Array<{ kind: string; case_node_id: number | null }>;
  factual_context?: Array<{ kind: string; case_node_id: number | null }>;
  legal_support?: Citation[];
  missing_requirement_count?: number;
  unmet_requirements?: number;
  evidence_count?: number;
};
type EvidenceKind = "plain_text" | "eml" | "pdf";
type DemoScenario = "data-access" | "digital-violence";

const state = {
  view: window.location.pathname === "/demo" ? "demo" : (localStorage.getItem("nexo-view") === "workspace" ? "workspace" : "home"),
  demoScenario: null as DemoScenario | null,
  demoEvidenceAdded: false,
  demoRequirementMet: false,
  demoEvaluated: false,
  demoPrepared: false,
  language: localStorage.getItem("nexo-language") === "es" ? "es" : "en",
  theme: localStorage.getItem("nexo-theme") === "light" ? "light" : "dark",
  apiBase: localStorage.getItem("nexo-api-base") || import.meta.env.VITE_NEXO_API_BASE || "",
  token: localStorage.getItem("nexo-token") ?? "",
  caseId: Number(localStorage.getItem("nexo-case-id")) || null,
  detail: null as CaseDetail | null,
  evaluation: null as Evaluation | null,
  evaluationId: null as number | null,
  preparation: null as Preparation | null,
};

function text(english: string, spanish: string): string {
  return state.language === "es" ? spanish : english;
}

function label(value: string): string {
  const labels: Record<string, [string, string]> = {
    artifact: ["artifact", "artefacto"],
    observation: ["observation", "observación"],
    user_assertion: ["user assertion", "afirmación de la usuaria"],
    derived_fact: ["derived fact", "hecho derivado"],
    inference: ["inference", "inferencia"],
    prepared: ["prepared", "preparada"],
    exported: ["exported", "exportada"],
    non_actionable: ["non-actionable", "no accionable"],
    insufficient_facts: ["insufficient facts", "hechos insuficientes"],
    out_of_jurisdiction: ["out of jurisdiction", "fuera de jurisdicción"],
    stale_policy: ["stale policy", "política desactualizada"],
    abstention: ["abstention", "abstención"],
    contraindicated: ["contraindicated", "contraindicada"],
    conflicting_legal_claims: ["conflicting legal claims", "afirmaciones legales en conflicto"],
  };
  const [english, spanish] = labels[value] ?? [value.replaceAll("_", " "), value.replaceAll("_", " ")];
  return text(english, spanish);
}

function applyTheme(): void {
  document.documentElement.dataset.theme = state.theme;
  document.documentElement.lang = state.language;
  document.title = state.language === "es" ? "NEXO — Espacio de casos" : "NEXO — Case workspace";
}

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("NEXO app root is missing");
const app: HTMLDivElement = root;

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#039;",
  })[character] ?? character);
}

function safeExternalUrl(value: string): string | null {
  try {
    const url = new URL(value, window.location.origin);
    return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
  } catch {
    return null;
  }
}

function endpoint(path: string): string {
  return `${state.apiBase.replace(/\/$/, "")}${path}`;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${state.token}`);
  if (init.body && !headers.has("content-type")) headers.set("content-type", "application/json");
  const response = await fetch(endpoint(path), { ...init, headers });
  if (!response.ok) {
    const detail = (await response.text()) || response.statusText;
    if (response.status === 401) throw new Error(text("Your workspace token is missing or invalid. Paste the bearer token supplied with your NEXO workspace, then save the connection.", "Falta el token del espacio de trabajo o no es válido. Pegá el token bearer que te entregaron para NEXO y guardá la conexión."));
    if (response.status === 403) throw new Error(text("This action is not allowed for the current workspace.", "Esta acción no está permitida para el espacio de trabajo actual."));
    throw new Error(`${response.status}: ${detail}`);
  }
  return response.json() as Promise<T>;
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? "");
      resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.onerror = () => reject(new Error(text("The file could not be read.", "No se pudo leer el archivo.")));
    reader.readAsDataURL(file);
  });
}

async function requestBytes(path: string): Promise<Blob> {
  const headers = new Headers({ Authorization: `Bearer ${state.token}` });
  const response = await fetch(endpoint(path), { headers });
  if (!response.ok) throw new Error(`${response.status}: ${(await response.text()) || response.statusText}`);
  return response.blob();
}

function downloadBlob(blob: Blob, filename: string): void {
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = filename;
  link.click();
  URL.revokeObjectURL(link.href);
}

function renderTimeline(): string {
  if (!state.detail) return `<p class="empty">${text("Create or load a case to see its evidence timeline.", "Creá o cargá un caso para ver su línea de evidencia.")}</p>`;
  if (state.detail.nodes.length === 0) return `<p class="empty">${text("This case has no graph nodes yet.", "Este caso todavía no tiene nodos.")}</p>`;
  return `<ol class="timeline">${state.detail.nodes.map((node) => `
    <li class="node node-${node.kind}">
      <span class="node-kind">${escapeHtml(label(node.kind))}</span>
      <strong>${text("Node", "Nodo")} ${node.node_id}</strong>
      <time datetime="${escapeHtml(node.created_at)}">${escapeHtml(new Date(node.created_at).toLocaleString())}</time>
      ${node.kind === "user_assertion" ? `<span>${node.confirmed ? text("Confirmed by the owner", "Confirmado por la titular") : text("Not confirmed", "No confirmado")}</span>` : ""}
    </li>`).join("")}</ol>`;
}

function renderEvidence(): string {
  const supports = state.evaluation?.factual_support ?? state.evaluation?.factual_context ?? [];
  if (!supports.length) return `<p class="muted">${text("No factual support rendered yet.", "Todavía no hay respaldo fáctico para mostrar.")}</p>`;
  return `<ul class="support-list">${supports.map((item) => `<li>${escapeHtml(item.kind)} · ${text("node", "nodo")} ${item.case_node_id ?? text("unresolved", "sin resolver")}</li>`).join("")}</ul>`;
}

function renderEvaluation(): string {
  const evaluation = state.evaluation;
  if (!evaluation) return `<p class="empty">${text("Run an evaluation to see whether an action is available.", "Ejecutá una evaluación para saber si hay una acción disponible.")}</p>`;
  const actionable = evaluation.kind === "actionable";
  const title = actionable ? (evaluation.status === "supported" ? text("Supported action", "Acción respaldada") : text("Conditionally supported", "Respaldada con condiciones")) : `${text("No action", "Sin acción")}: ${label(evaluation.variant ?? "non_actionable")}`;
  return `<article class="evaluation ${actionable && evaluation.available ? "positive" : "caution"}">
    <div class="evaluation-heading"><span class="eyebrow">${actionable ? text("ACTION OPTION", "OPCIÓN DE ACCIÓN") : text("HONEST NEGATIVE", "NEGATIVA HONESTA")}</span><h3>${escapeHtml(title)}</h3></div>
    <p>${actionable ? (evaluation.available ? text("This route is available from the recorded support.", "Esta vía está disponible según el respaldo registrado.") : text("This route is not available until its requirements are met.", "Esta vía no está disponible hasta cumplir sus requisitos.")) : escapeHtml(evaluation.cause ? `${text("Reason", "Motivo")}: ${label(evaluation.cause)}.` : text("The current case does not support an available action.", "El caso actual no respalda una acción disponible."))}</p>
    <h4>${text("Why this is shown", "Por qué aparece esto")}</h4>${renderEvidence()}
    ${evaluation.legal_support?.length ? `<h4>${text("Legal support", "Respaldo legal")}</h4><ul class="citation-list">${evaluation.legal_support.map((citation) => { const sourceUrl = safeExternalUrl(citation.source_locator); return `<li><strong>${escapeHtml(citation.proposition)}</strong><span>${escapeHtml(citation.source_issuer)}</span>${sourceUrl ? `<a href="${escapeHtml(sourceUrl)}" target="_blank" rel="noreferrer">${text("Open captured source", "Abrir fuente capturada")}</a>` : `<span class="muted">${text("Captured source has no safe web URL", "La fuente capturada no tiene una URL web segura")}</span>`}</li>`; }).join("")}</ul>` : ""}
  </article>`;
}

function renderPreparation(): string {
  const evaluation = state.evaluation;
  if (!evaluation?.available || !state.evaluationId) {
    return `<p class="muted">${text("A preparation appears only after the current evaluation returns an available action.", "La preparación aparece cuando la evaluación devuelve una acción disponible.")}</p>`;
  }
  const preparation = state.preparation;
  return `<div class="preparation-card">
    <p class="muted">${text("This creates local, deterministic material. It does not send, file, sign, or notify anyone.", "Esto crea material local y determinista. No envía, presenta, firma ni notifica a nadie.")}</p>
    ${preparation ? `<p><strong>${text("Preparation", "Preparación")} ${preparation.preparation_id}</strong> · <span class="badge">${escapeHtml(label(preparation.status))}</span>${preparation.manifest_digest ? `<br><span class="digest">${text("Manifest", "Manifiesto")} ${escapeHtml(preparation.manifest_digest)}</span>` : ""}</p>` : `<p class="muted">${text("No draft prepared for this evaluation yet.", "Todavía no hay un borrador preparado para esta evaluación.")}</p>`}
    <div class="button-row">
      <button id="prepare" class="secondary">${preparation ? text("Refresh draft", "Actualizar borrador") : text("Prepare draft", "Preparar borrador")}</button>
      ${preparation ? `<button id="export"${preparation.status === "exported" ? " class=\"secondary\"" : ""}>${preparation.status === "exported" ? text("Verify export", "Verificar exportación") : text("Export locally", "Exportar localmente")}</button>` : ""}
      ${preparation?.status === "exported" ? `<button id="download-manifest" class="secondary">${text("Download manifest", "Descargar manifiesto")}</button><button id="download-artifact" class="secondary">${text("Download artifact", "Descargar artefacto")}</button>` : ""}
    </div>
  </div>`;
}

function renderLanding(): void {
  applyTheme();
  app.innerHTML = `<main class="landing shell">
    <header class="topbar"><div class="brand"><img src="${logoUrl}" alt="NEXO" /><div><span class="eyebrow">${text("A CLEARER PLACE TO BEGIN", "UN LUGAR MÁS CLARO PARA EMPEZAR")}</span><h1>NEXO</h1></div></div><nav class="topbar-actions" aria-label="${text("Main navigation", "Navegación principal")}"><button id="open-demo" class="utility-button" type="button">${text("DEMO", "DEMO")}</button><button id="open-guide" class="utility-button" type="button">${text("Step-by-step guide", "Guía paso a paso")}</button><button id="language-toggle" class="utility-button" type="button">${state.language === "en" ? "ES" : "EN"}</button><button id="theme-toggle" class="utility-button" type="button">${state.theme === "dark" ? text("Light mode", "Modo claro") : text("Dark mode", "Modo oscuro")}</button></nav></header>
    <section class="hero panel"><div class="hero-copy"><span class="eyebrow">${text("WHEN SOMETHING IMPORTANT HAPPENS", "CUANDO PASA ALGO IMPORTANTE")}</span><h2>${text("Turn what happened into something you can show.", "Convertí lo que pasó en algo que podés mostrar.")}</h2><p class="hero-lead">${text("NEXO helps you gather messages, emails, documents and your own account in one calm, private workspace.", "NEXO te ayuda a reunir mensajes, correos, documentos y tu propio relato en un espacio privado y tranquilo.")}</p><div class="hero-actions"><button id="start-workspace" type="button">${text("Open my workspace", "Abrir mi espacio")}</button><a class="text-link" href="#examples">${text("Explore examples", "Ver ejemplos")}</a></div></div><div class="hero-note" aria-label="${text("What NEXO does", "Qué hace NEXO")}"><span class="note-mark">✦</span><p>${text("You remain in control. NEXO prepares information; it never sends anything or acts for you.", "Vos seguís teniendo el control. NEXO prepara la información; nunca envía nada ni actúa por vos.")}</p></div></section>
    <section id="how-it-works" class="landing-section"><span class="eyebrow">${text("A SIMPLE PATH", "UN CAMINO SIMPLE")}</span><h2>${text("From scattered material to a clear next step.", "De material disperso a un próximo paso claro.")}</h2><div class="steps"><article><span>01</span><h3>${text("Gather", "Reuní")}</h3><p>${text("Add a message, an email, a PDF, or your own account of what happened.", "Agregá un mensaje, un correo, un PDF o tu propio relato de lo que pasó.")}</p></article><article><span>02</span><h3>${text("Understand", "Entendé")}</h3><p>${text("See what is directly supported, what is still missing, and why NEXO says so.", "Mirá qué está respaldado, qué falta y por qué NEXO lo dice.")}</p></article><article><span>03</span><h3>${text("Prepare", "Prepará")}</h3><p>${text("When there is a supported path, create materials you can review and choose to send yourself.", "Cuando hay un camino respaldado, prepará materiales que vos revisás y decidís si enviar.")}</p></article></div></section>
    <section id="examples" class="landing-section examples"><span class="eyebrow">${text("SEE IT WITHOUT A TOKEN", "MIRALO SIN TOKEN")}</span><h2>${text("Examples of what a case can look like.", "Ejemplos de cómo puede verse un caso.")}</h2><p class="muted">${text("These are guided examples, not real people or legal advice. They are here so you can understand the experience before connecting a private workspace.", "Son ejemplos guiados, no personas reales ni asesoramiento legal. Están para que entiendas la experiencia antes de conectar un espacio privado.")}</p><div class="example-grid"><article class="example-card"><span class="example-tag">${text("PERSONAL DATA", "DATOS PERSONALES")}</span><h3>${text("A request to see your data", "Un pedido para conocer tus datos")}</h3><p>${text("A person saves an email asking an organization what personal information it holds about them.", "Una persona guarda un correo en el que pide a una organización saber qué datos personales tiene sobre ella.")}</p><div class="example-result"><strong>${text("What NEXO shows", "Qué muestra NEXO")}</strong><span>${text("Identity confirmed · email preserved · legal source cited", "Identidad confirmada · correo preservado · fuente legal citada")}</span></div></article><article class="example-card"><span class="example-tag">${text("DIGITAL VIOLENCE", "VIOLENCIA DIGITAL")}</span><h3>${text("Content shared without consent", "Contenido difundido sin consentimiento")}</h3><p>${text("A person preserves a message and a document identifying where intimate content was published.", "Una persona conserva un mensaje y un documento que identifican dónde se publicó contenido íntimo.")}</p><div class="example-result"><strong>${text("What NEXO shows", "Qué muestra NEXO")}</strong><span>${text("Content identified · evidence kept · a possible route explained", "Contenido identificado · evidencia conservada · posible vía explicada")}</span></div></article></div><p class="example-note">${text("No example creates a real case or sends anything. To work with your own evidence, use “Open my workspace” and ask the workspace administrator for your private token.", "Ningún ejemplo crea un caso real ni envía nada. Para trabajar con tu propia evidencia, usá “Abrir mi espacio” y pedile tu token privado a quien administra el espacio.")}</p></section>
    <section class="promise-grid"><article class="promise-card"><span class="eyebrow">${text("HONEST BY DESIGN", "HONESTO POR DISEÑO")}</span><h3>${text("No invented certainty.", "No inventa certezas.")}</h3><p>${text("If the information is not enough, NEXO says what is missing instead of pretending to know.", "Si la información no alcanza, NEXO muestra qué falta en vez de fingir que sabe.")}</p></article><article class="promise-card"><span class="eyebrow">${text("YOUR DECISION", "TU DECISIÓN")}</span><h3>${text("Nothing is sent for you.", "No envía nada por vos.")}</h3><p>${text("NEXO can prepare a request or evidence package. A person always decides what happens next.", "NEXO puede preparar un pedido o un paquete de evidencia. Una persona siempre decide qué sigue.")}</p></article><article class="promise-card"><span class="eyebrow">${text("CHECKABLE", "COMPROBABLE")}</span><h3>${text("Your files keep their identity.", "Tus archivos conservan su identidad.")}</h3><p>${text("A SHA-256 fingerprint helps show that the file you saved is the same one used later. It does not judge your story.", "Una huella SHA-256 ayuda a mostrar que el archivo guardado es el mismo que se usó después. No juzga tu historia.")}</p></article></section>
    <section class="landing-footer panel"><div><h2>${text("You do not need to understand the technology to begin.", "No necesitás entender la tecnología para empezar.")}</h2><p class="muted">${text("The technical details exist for people who need to inspect them. This space is for you and what you need to preserve.", "Los detalles técnicos existen para quienes necesitan inspeccionarlos. Este espacio es para vos y para lo que necesitás conservar.")}</p></div><button id="start-workspace-bottom" class="secondary" type="button">${text("Open my workspace", "Abrir mi espacio")}</button></section>
    <footer><span>${text("NEXO is not a lawyer or an emergency service. If you are in immediate danger, contact local emergency support.", "NEXO no es un abogado ni un servicio de emergencias. Si estás en peligro inmediato, contactá a los servicios de emergencia locales.")}</span></footer>
  </main>`;
  bindLandingEvents();
}

function renderGuide(): void {
  applyTheme();
  app.innerHTML = `<main class="guide-page shell">
    <header class="topbar"><div class="brand"><img src="${logoUrl}" alt="NEXO" /><div><span class="eyebrow">${text("GETTING STARTED", "PRIMEROS PASOS")}</span><h1>NEXO</h1></div></div><nav class="topbar-actions" aria-label="${text("Main navigation", "Navegación principal")}"><button id="back-home" class="utility-button" type="button">${text("Home", "Inicio")}</button><button id="language-toggle" class="utility-button" type="button">${state.language === "en" ? "ES" : "EN"}</button><button id="theme-toggle" class="utility-button" type="button">${state.theme === "dark" ? text("Light mode", "Modo claro") : text("Dark mode", "Modo oscuro")}</button></nav></header>
    <section class="guide-intro panel"><span class="eyebrow">${text("NO TECHNICAL KNOWLEDGE REQUIRED", "NO NECESITÁS CONOCIMIENTOS TÉCNICOS")}</span><h2>${text("How to go from the public page to your first private case.", "Cómo pasar de la página pública a tu primer caso privado.")}</h2><p>${text("This guide explains who provides the token, where it goes, and what each step means. If you only want to understand NEXO, you can explore the examples without connecting anything.", "Esta guía explica quién entrega el token, dónde se coloca y qué significa cada paso. Si solo querés conocer NEXO, podés mirar los ejemplos sin conectar nada.")}</p></section>
    <section class="guide-steps"><article class="guide-step"><span class="step-number">1</span><div><h3>${text("Ask for access to a private workspace", "Pedí acceso a un espacio privado")}</h3><p>${text("NEXO is not a public account system yet. A workspace administrator deploys the API and creates its first owner credential. Ask that person for two things: the API address and your private bearer token.", "NEXO todavía no es un sistema de cuentas públicas. Una persona administradora despliega la API y crea la primera credencial del espacio. Pedile dos cosas: la dirección de la API y tu token bearer privado.")}</p><div class="guide-callout"><strong>${text("The token comes from the administrator", "El token lo entrega la persona administradora")}</strong><span>${text("It is generated during deployment from NEXO_BOOTSTRAP_OWNER. It should be sent privately, never pasted into a public issue, chat, screenshot, or source code.", "Se genera durante el deployment a partir de NEXO_BOOTSTRAP_OWNER. Debe enviarse de forma privada, nunca publicarse en un issue, chat, captura de pantalla o código fuente.")}</span></div></div></article><article class="guide-step"><span class="step-number">2</span><div><h3>${text("Connect the web page", "Conectá la página web")}</h3><p>${text("Choose “Open my workspace” and enter the API base and workspace token. The browser keeps them in your local storage for this device. NEXO sends the token as a Bearer authorization header; it does not display it back in the page.", "Elegí “Abrir mi espacio” e ingresá la base de API y el token del espacio. El navegador los guarda en el almacenamiento local de este dispositivo. NEXO envía el token como autorización Bearer; no lo vuelve a mostrar en la página.")}</p><ol class="guide-list"><li>${text("API base: the HTTPS address supplied by the administrator.", "Base de API: la dirección HTTPS que te dio la persona administradora.")}</li><li>${text("Workspace token: the private token supplied to you.", "Token del espacio: el token privado que te entregaron.")}</li><li>${text("Save connection, then choose “Open my workspace”.", "Guardá la conexión y elegí “Abrir mi espacio”.")}</li></ol></div></article><article class="guide-step"><span class="step-number">3</span><div><h3>${text("Create a case", "Creá un caso")}</h3><p>${text("Press “New case”. NEXO creates an empty case owned by the authenticated workspace owner. Nothing is public and nothing is sent outside the workspace.", "Presioná “Nuevo caso”. NEXO crea un caso vacío perteneciente a la persona autenticada. Nada se hace público y nada se envía fuera del espacio.")}</p></div></article><article class="guide-step"><span class="step-number">4</span><div><h3>${text("Add what you have", "Agregá lo que tenés")}</h3><p>${text("You can paste text or upload a plain-text file, an email saved as .eml, or a PDF. NEXO keeps the original bytes and extracts readable observations in an isolated process. A failed extraction stays visible as a rejection; it is not silently treated as proof.", "Podés pegar texto o subir un archivo de texto, un correo guardado como .eml o un PDF. NEXO conserva los bytes originales y extrae observaciones legibles en un proceso aislado. Si la extracción falla, queda visible como rechazo; no se trata silenciosamente como prueba.")}</p></div></article><article class="guide-step"><span class="step-number">5</span><div><h3>${text("Evaluate and prepare", "Evaluá y prepará")}</h3><p>${text("Evaluation explains whether the current evidence supports a route and what is missing. Preparation creates a document or evidence package for you to review. NEXO stops there: a human decides whether anything is sent.", "La evaluación explica si la evidencia actual respalda una vía y qué falta. La preparación crea un documento o paquete de evidencia para que lo revises. NEXO termina ahí: una persona decide si se envía algo.")}</p></div></article></section>
    <section class="guide-troubleshooting panel"><span class="eyebrow">${text("IF SOMETHING GOES WRONG", "SI ALGO FALLA")}</span><h2>${text("The most common messages", "Los mensajes más comunes")}</h2><details open><summary><code>401: expected Bearer token</code></summary><p>${text("The page did not receive a token, or the token is not valid for this API. Return to the connection form, paste the private token exactly as supplied, save it, and try again. If you do not have one, ask the workspace administrator; do not invent one.", "La página no recibió un token o el token no es válido para esta API. Volvé al formulario de conexión, pegá el token privado exactamente como te lo dieron, guardalo e intentá de nuevo. Si no tenés uno, pedíselo a la persona administradora; no intentes inventarlo.")}</p></details><details><summary>${text("The API cannot be reached", "No se puede conectar con la API")}</summary><p>${text("Check that the API base starts with the correct HTTPS address and that the administrator says the service is running. A public frontend by itself cannot create cases.", "Revisá que la base de API empiece con la dirección HTTPS correcta y que la persona administradora confirme que el servicio está funcionando. Una página pública sola no puede crear casos.")}</p></details><details><summary>${text("The file was rejected", "El archivo fue rechazado")}</summary><p>${text("The original file remains recorded, but NEXO could not safely extract readable observations from it. Try the correct evidence type or preserve the rejection and add another source.", "El archivo original queda registrado, pero NEXO no pudo extraer observaciones legibles de forma segura. Probá con el tipo de evidencia correcto o conservá el rechazo y agregá otra fuente.")}</p></details></section>
    <section class="guide-actions"><button id="start-workspace-guide" type="button">${text("Open my workspace", "Abrir mi espacio")}</button><button id="back-home-bottom" class="secondary" type="button">${text("Back to examples", "Volver a los ejemplos")}</button></section>
  </main>`;
  bindGuideEvents();
}

function renderDemo(): void {
  applyTheme();
  const scenario = state.demoScenario;
  const header = `<header class="topbar"><div class="brand"><img src="${logoUrl}" alt="NEXO" /><div><span class="eyebrow">${text("INTERACTIVE DEMO", "DEMO INTERACTIVA")}</span><h1>NEXO</h1></div></div><nav class="topbar-actions" aria-label="${text("Demo navigation", "Navegación de la demo")}"><button id="demo-home" class="utility-button" type="button">${text("Home", "Inicio")}</button><button id="demo-language" class="utility-button" type="button">${state.language === "en" ? "ES" : "EN"}</button><button id="demo-theme" class="utility-button" type="button">${state.theme === "dark" ? text("Light mode", "Modo claro") : text("Dark mode", "Modo oscuro")}</button></nav></header>`;
  if (!scenario) {
    app.innerHTML = `<main class="demo-page shell">${header}<section class="demo-intro panel"><span class="eyebrow">${text("NO TOKEN · NO REAL DATA · FULL FLOW", "SIN TOKEN · SIN DATOS REALES · FLUJO COMPLETO")}</span><h2>${text("Try the cases as a person or judge would.", "Probá los casos como lo haría una persona o un juez.")}</h2><p>${text("Choose a guided case and complete its steps. This demo runs in your browser only. It does not contact the API, save personal data, or send anything.", "Elegí un caso guiado y completá sus pasos. Esta demo funciona solo en tu navegador. No contacta la API, no guarda datos personales ni envía nada.")}</p></section><section class="demo-choice-grid"><article class="demo-choice"><span class="example-tag">${text("CASE 01 · PERSONAL DATA", "CASO 01 · DATOS PERSONALES")}</span><h3>${text("Request to see your personal data", "Pedido para conocer tus datos personales")}</h3><p>${text("Follow an email from a person asking an organization what information it holds about them.", "Seguí el correo de una persona que pide a una organización saber qué información tiene sobre ella.")}</p><button id="choose-data-access" type="button">${text("Start this demo", "Empezar esta demo")}</button></article><article class="demo-choice"><span class="example-tag">${text("CASE 02 · DIGITAL VIOLENCE", "CASO 02 · VIOLENCIA DIGITAL")}</span><h3>${text("Content shared without consent", "Contenido difundido sin consentimiento")}</h3><p>${text("Preserve a message and identify the place where intimate content was published.", "Conservá un mensaje e identificá el lugar donde se publicó contenido íntimo.")}</p><button id="choose-digital-violence" type="button">${text("Start this demo", "Empezar esta demo")}</button></article></section><p class="demo-disclaimer">${text("Demo only: every item is fictional and reset when you leave this page.", "Solo demo: todos los elementos son ficticios y se reinician cuando salís de esta página.")}</p></main>`;
  } else {
    const dataAccess = scenario === "data-access";
    const title = dataAccess ? text("Request to see your personal data", "Pedido para conocer tus datos personales") : text("Content shared without consent", "Contenido difundido sin consentimiento");
    const evidenceLabel = dataAccess ? text("Example email added", "Correo de ejemplo agregado") : text("Example message and PDF added", "Mensaje y PDF de ejemplo agregados");
    const requirementLabel = dataAccess ? text("Identity confirmed", "Identidad confirmada") : text("Content location identified", "Ubicación del contenido identificada");
    const result = dataAccess ? text("A request route is supported by the recorded email and confirmed identity.", "El pedido está respaldado por el correo registrado y la identidad confirmada.") : text("A possible content-removal route is supported because the specific content location was identified.", "Una posible vía de remoción está respaldada porque se identificó dónde está el contenido.");
    app.innerHTML = `<main class="demo-page shell">${header}<section class="demo-workspace panel"><div class="demo-heading"><div><span class="eyebrow">${text("GUIDED CASE", "CASO GUIADO")}</span><h2>${title}</h2></div><button id="choose-another-demo" class="secondary" type="button">${text("Choose another case", "Elegir otro caso")}</button></div><div class="demo-banner">${text("You are in a safe example. Nothing here reaches the real backend.", "Estás en un ejemplo seguro. Nada de acá llega al backend real.")}</div><ol class="demo-progress"><li class="${state.demoEvidenceAdded ? "done" : "active"}"><strong>1</strong><span>${text("Add evidence", "Agregar evidencia")}</span></li><li class="${state.demoRequirementMet ? "done" : state.demoEvidenceAdded ? "active" : ""}"><strong>2</strong><span>${requirementLabel}</span></li><li class="${state.demoEvaluated ? "done" : state.demoRequirementMet ? "active" : ""}"><strong>3</strong><span>${text("Evaluate", "Evaluar")}</span></li><li class="${state.demoPrepared ? "done" : state.demoEvaluated ? "active" : ""}"><strong>4</strong><span>${text("Prepare", "Preparar")}</span></li></ol><section class="demo-step-card"><span class="eyebrow">${text("YOUR NEXT STEP", "TU PRÓXIMO PASO")}</span>${!state.demoEvidenceAdded ? `<h3>${text("Start by adding the example evidence", "Empezá agregando la evidencia de ejemplo")}</h3><p>${dataAccess ? text("This is a fictional .eml email asking an organization for access to personal data.", "Este es un correo .eml ficticio que pide a una organización acceso a datos personales.") : text("This is a fictional message plus a PDF record identifying where content was published.", "Este es un mensaje ficticio más un PDF que identifica dónde se publicó el contenido.")}</p><button id="demo-add-evidence" type="button">${text("Add example evidence", "Agregar evidencia de ejemplo")}</button>` : !state.demoRequirementMet ? `<h3>${requirementLabel}</h3><p>${dataAccess ? text("In a real case, this is where the owner confirms a fact about their identity. In this demo, you can see the same step without sharing anything.", "En un caso real, acá la titular confirma un hecho sobre su identidad. En esta demo podés ver el mismo paso sin compartir nada.") : text("In a real case, this is where the specific URL or place containing the content becomes part of the evidence graph.", "En un caso real, acá la URL o el lugar específico del contenido pasa a formar parte del grafo de evidencia.")}</p><button id="demo-meet-requirement" type="button">${dataAccess ? text("Confirm identity", "Confirmar identidad") : text("Identify content location", "Identificar ubicación")}</button>` : !state.demoEvaluated ? `<h3>${text("The case is ready to evaluate", "El caso está listo para evaluar")}</h3><p>${text("NEXO will show what is supported and why. It will not invent a conclusion if the evidence is insufficient.", "NEXO va a mostrar qué está respaldado y por qué. No va a inventar una conclusión si la evidencia no alcanza.")}</p><button id="demo-evaluate" type="button">${text("Evaluate example case", "Evaluar caso de ejemplo")}</button>` : !state.demoPrepared ? `<div class="demo-result"><span class="eyebrow">${text("SUPPORTED EXAMPLE RESULT", "RESULTADO DE EJEMPLO RESPALDADO")}</span><h3>${text("A possible next step is available", "Hay un posible próximo paso")}</h3><p>${result}</p><ul><li>${evidenceLabel}</li><li>${requirementLabel}</li><li>${text("Source-backed explanation", "Explicación respaldada por una fuente")}</li></ul></div><button id="demo-prepare" type="button">${text("Prepare example materials", "Preparar materiales de ejemplo")}</button>` : `<div class="demo-result positive"><span class="eyebrow">${text("DEMO COMPLETE", "DEMO COMPLETA")}</span><h3>${text("The materials are ready to review", "Los materiales están listos para revisar")}</h3><p>${text("This is where NEXO stops. In a real case, a person reviews the material and decides what to do next.", "Acá termina NEXO. En un caso real, una persona revisa el material y decide qué hacer después.")}</p><button id="demo-restart" class="secondary" type="button">${text("Run it again", "Repetir demo")}</button></div>`}</section></section></main>`;
  }
  bindDemoEvents();
}

function bindDemoEvents(): void {
  const goHome = () => { state.view = "home"; state.demoScenario = null; window.history.pushState({}, "", "/"); localStorage.setItem("nexo-view", "home"); render(); };
  const choose = (scenario: DemoScenario) => { state.view = "demo"; state.demoScenario = scenario; state.demoEvidenceAdded = false; state.demoRequirementMet = false; state.demoEvaluated = false; state.demoPrepared = false; window.history.pushState({}, "", "/demo"); renderDemo(); };
  document.querySelector<HTMLButtonElement>("#demo-home")?.addEventListener("click", goHome);
  document.querySelector<HTMLButtonElement>("#demo-language")?.addEventListener("click", () => { state.language = state.language === "en" ? "es" : "en"; localStorage.setItem("nexo-language", state.language); renderDemo(); });
  document.querySelector<HTMLButtonElement>("#demo-theme")?.addEventListener("click", () => { state.theme = state.theme === "dark" ? "light" : "dark"; localStorage.setItem("nexo-theme", state.theme); renderDemo(); });
  document.querySelector<HTMLButtonElement>("#choose-data-access")?.addEventListener("click", () => choose("data-access"));
  document.querySelector<HTMLButtonElement>("#choose-digital-violence")?.addEventListener("click", () => choose("digital-violence"));
  document.querySelector<HTMLButtonElement>("#choose-another-demo")?.addEventListener("click", () => { state.demoScenario = null; renderDemo(); });
  document.querySelector<HTMLButtonElement>("#demo-add-evidence")?.addEventListener("click", () => { state.demoEvidenceAdded = true; renderDemo(); });
  document.querySelector<HTMLButtonElement>("#demo-meet-requirement")?.addEventListener("click", () => { state.demoRequirementMet = true; renderDemo(); });
  document.querySelector<HTMLButtonElement>("#demo-evaluate")?.addEventListener("click", () => { state.demoEvaluated = true; renderDemo(); });
  document.querySelector<HTMLButtonElement>("#demo-prepare")?.addEventListener("click", () => { state.demoPrepared = true; renderDemo(); });
  document.querySelector<HTMLButtonElement>("#demo-restart")?.addEventListener("click", () => choose(state.demoScenario ?? "data-access"));
}

function bindLandingEvents(): void {
  document.querySelector<HTMLButtonElement>("#language-toggle")?.addEventListener("click", () => { state.language = state.language === "en" ? "es" : "en"; localStorage.setItem("nexo-language", state.language); renderLanding(); });
  document.querySelector<HTMLButtonElement>("#theme-toggle")?.addEventListener("click", () => { state.theme = state.theme === "dark" ? "light" : "dark"; localStorage.setItem("nexo-theme", state.theme); renderLanding(); });
  document.querySelector<HTMLButtonElement>("#open-guide")?.addEventListener("click", () => { state.view = "guide"; localStorage.setItem("nexo-view", "guide"); render(); });
  document.querySelector<HTMLButtonElement>("#open-demo")?.addEventListener("click", () => { state.view = "demo"; localStorage.setItem("nexo-view", "demo"); window.history.pushState({}, "", "/demo"); render(); });
  const openWorkspace = () => { state.view = "workspace"; localStorage.setItem("nexo-view", "workspace"); render(); document.querySelector<HTMLInputElement>("#token")?.focus(); };
  document.querySelector<HTMLButtonElement>("#start-workspace")?.addEventListener("click", openWorkspace);
  document.querySelector<HTMLButtonElement>("#start-workspace-bottom")?.addEventListener("click", openWorkspace);
}

function bindGuideEvents(): void {
  document.querySelector<HTMLButtonElement>("#language-toggle")?.addEventListener("click", () => { state.language = state.language === "en" ? "es" : "en"; localStorage.setItem("nexo-language", state.language); renderGuide(); });
  document.querySelector<HTMLButtonElement>("#theme-toggle")?.addEventListener("click", () => { state.theme = state.theme === "dark" ? "light" : "dark"; localStorage.setItem("nexo-theme", state.theme); renderGuide(); });
  const backHome = () => { state.view = "home"; localStorage.setItem("nexo-view", "home"); render(); };
  const openWorkspace = () => { state.view = "workspace"; localStorage.setItem("nexo-view", "workspace"); render(); document.querySelector<HTMLInputElement>("#token")?.focus(); };
  document.querySelector<HTMLButtonElement>("#back-home")?.addEventListener("click", backHome);
  document.querySelector<HTMLButtonElement>("#back-home-bottom")?.addEventListener("click", backHome);
  document.querySelector<HTMLButtonElement>("#start-workspace-guide")?.addEventListener("click", openWorkspace);
}

function render(): void {
  if (state.view === "home") { renderLanding(); return; }
  if (state.view === "guide") { renderGuide(); return; }
  if (state.view === "demo") { renderDemo(); return; }
  applyTheme();
  app.innerHTML = `<main class="shell">
    <header class="topbar"><div class="brand"><img src="${logoUrl}" alt="NEXO" /><div><span class="eyebrow">${text("VERIFIABLE DIGITAL-RIGHTS CASE GRAPH", "GRAFO VERIFICABLE DE DERECHOS DIGITALES")}</span><h1>NEXO</h1></div></div><div class="topbar-actions"><span class="status-dot">${text("Local workspace", "Espacio local")}</span><button id="language-toggle" class="utility-button" type="button">${state.language === "en" ? "ES" : "EN"}</button><button id="theme-toggle" class="utility-button" type="button">${state.theme === "dark" ? text("Light mode", "Modo claro") : text("Dark mode", "Modo oscuro")}</button></div></header>
    <section class="intro panel"><span class="eyebrow">${text("A CALM PLACE TO START", "UN ESPACIO TRANQUILO PARA EMPEZAR")}</span><h2>${text("Keep what happened, understand what can be done.", "Guardá lo que pasó y entendé qué se puede hacer.")}</h2><p>${text("NEXO helps you organize messages, emails and PDFs into a private, verifiable case. It does not judge you, contact anyone, or send a legal filing.", "NEXO te ayuda a ordenar mensajes, correos y PDFs en un caso privado y verificable. No te juzga, no contacta a nadie y no presenta trámites legales.")}</p><details><summary>${text("Why does NEXO mention SHA-256?", "¿Por qué NEXO menciona SHA-256?")}</summary><p>${text("SHA-256 creates a unique-looking fingerprint of each file. NEXO uses it to show that the evidence you saved is the same evidence later used in the case. It does not reveal the file contents and it is not a measure of whether your story is true.", "SHA-256 crea una huella digital de cada archivo. NEXO la usa para mostrar que la evidencia guardada es la misma que después se usó en el caso. No revela el contenido del archivo ni mide si tu historia es verdadera.")}</p></details></section>
    <section class="setup panel"><div><h2>${text("Connect your workspace", "Conectá tu espacio de trabajo")}</h2><p class="muted">${text("Before creating a case, paste the bearer token provided for your workspace. It is like a private key: keep it to yourself.", "Antes de crear un caso, pegá el token bearer de tu espacio de trabajo. Es como una llave privada: guardalo para vos.")}</p></div><form id="connection-form" class="connection-form">
      <label>${text("API base", "Base de API")} <input id="api-base" value="${escapeHtml(state.apiBase)}" placeholder="Same origin or http://127.0.0.1:8080" /></label>
      <label>${text("Workspace token", "Token del espacio de trabajo")} <input id="token" type="password" value="${escapeHtml(state.token)}" autocomplete="off" required aria-describedby="token-help" /> <span id="token-help" class="field-help">${text("The API needs this token to create and read your cases.", "La API necesita este token para crear y leer tus casos.")}</span></label>
      <button type="submit">${text("Save connection", "Guardar conexión")}</button>
    </form></section>
    <section class="workspace-grid">
      <div class="column"><section class="panel"><div class="section-heading"><div><span class="eyebrow">${text("CASE", "CASO")}</span><h2>${state.caseId ? `${text("Case", "Caso")} ${state.caseId}` : text("Start a case", "Iniciar un caso")}</h2></div><button id="new-case" class="secondary">${text("New case", "Nuevo caso")}</button></div><div id="case-feedback" class="feedback" aria-live="polite"></div>${renderTimeline()}</section>
      <section class="panel"><span class="eyebrow">${text("EVIDENCE INTAKE", "INGRESO DE EVIDENCIA")}</span><h2>${text("Add what happened", "Agregá lo que pasó")}</h2><p class="muted">${text("You can paste text or choose an email/PDF. The original file is kept with its SHA-256 fingerprint.", "Podés pegar texto o elegir un correo/PDF. El archivo original se conserva con su huella SHA-256.")}</p><form id="evidence-form"><label>${text("Evidence type", "Tipo de evidencia")} <select id="evidence-kind" aria-describedby="evidence-kind-help"><option value="plain_text">${text("Text message or note", "Mensaje o nota de texto")}</option><option value="eml">${text("Email (.eml)", "Correo electrónico (.eml)")}</option><option value="pdf">${text("PDF document", "Documento PDF")}</option></select></label><p id="evidence-kind-help" class="field-help">${text("Choose the type that matches the evidence. This helps NEXO read it safely.", "Elegí el tipo que corresponde a la evidencia. Así NEXO puede leerla de forma segura.")}</p><label>${text("Choose a file (optional)", "Elegí un archivo (opcional)")} <input id="evidence-file" type="file" accept=".txt,.eml,.pdf,text/plain,message/rfc822,application/pdf" /></label><label>${text("Or paste text", "O pegá el texto")} <textarea id="evidence-text" rows="7" placeholder="${text("Paste the relevant evidence here", "Pegá aquí la evidencia relevante")}" aria-describedby="evidence-text-help"></textarea><span id="evidence-text-help" class="field-help">${text("For a PDF, choose the file above. You do not need to understand SHA-256.", "Para un PDF, elegí el archivo de arriba. No necesitás entender SHA-256.")}</span></label><button type="submit">${text("Save evidence", "Guardar evidencia")}</button></form><p class="muted small">${text("NEXO stores the original bytes and extracts readable observations in a sandbox. If a file cannot be read, it remains visible as a bounded rejection.", "NEXO guarda los bytes originales y extrae observaciones legibles en un entorno aislado. Si no puede leer un archivo, queda visible como un rechazo explicado.")}</p></section></div>
      <div class="column"><section class="panel"><span class="eyebrow">${text("OWNER ASSERTION", "AFIRMACIÓN DE LA TITULAR")}</span><h2>${text("Confirm a fact", "Confirmá un hecho")}</h2><p class="muted">${text("A confirmation is a user assertion, not an extracted observation.", "Una confirmación es una afirmación de la usuaria, no una observación extraída.")}</p><button id="confirm-assertion" class="secondary" type="button">${text("Record confirmed assertion", "Registrar afirmación confirmada")}</button></section>
      <section class="panel"><div class="section-heading"><div><span class="eyebrow">${text("EVALUATION", "EVALUACIÓN")}</span><h2>${text("Why is this shown?", "¿Por qué aparece esto?")}</h2></div><button id="evaluate" type="button">${text("Evaluate case", "Evaluar caso")}</button></div><div id="evaluation-output" aria-live="polite">${renderEvaluation()}</div><div class="preparation-section"><span class="eyebrow">${text("PREPARATION / EXPORT", "PREPARACIÓN / EXPORTACIÓN")}</span><h3>${text("Keep the proof portable", "Conservá la prueba portable")}</h3>${renderPreparation()}</div></section></div>
    </section><footer><span>${text("NEXO stops at preparation. A human remains the actor for any external legal act.", "NEXO se detiene en la preparación. Una persona sigue siendo responsable de cualquier acto legal externo.")}</span></footer>
  </main>`;
  bindEvents();
}

function feedback(message: string, error = false): void {
  const target = document.querySelector<HTMLDivElement>("#case-feedback");
  if (target) {
    target.textContent = message;
    target.className = `feedback ${error ? "error" : "success"}`;
    target.setAttribute("role", error ? "alert" : "status");
  }
}

async function whileBusy<T>(button: HTMLButtonElement, label: string, task: () => Promise<T>): Promise<T> {
  const originalLabel = button.textContent ?? text("Working", "Procesando");
  button.disabled = true;
  button.setAttribute("aria-busy", "true");
  button.textContent = label;
  try {
    return await task();
  } finally {
    button.disabled = false;
    button.removeAttribute("aria-busy");
    button.textContent = originalLabel;
  }
}

async function loadCase(): Promise<void> {
  if (!state.caseId) return;
  const [detail, evaluations] = await Promise.all([
    request<CaseDetail>(`/v1/cases/${state.caseId}`),
    request<EvaluationRecord[]>(`/v1/cases/${state.caseId}/evaluations`),
  ]);
  state.detail = detail;
  state.evaluationId = evaluations[0]?.evaluation_id ?? null;
  state.evaluation = evaluations[0]?.result ?? null;
  state.preparation = null;
  render();
}

function bindEvents(): void {
  document.querySelector<HTMLButtonElement>("#language-toggle")?.addEventListener("click", () => {
    state.language = state.language === "en" ? "es" : "en";
    localStorage.setItem("nexo-language", state.language);
    render();
  });
  document.querySelector<HTMLButtonElement>("#theme-toggle")?.addEventListener("click", () => {
    state.theme = state.theme === "dark" ? "light" : "dark";
    localStorage.setItem("nexo-theme", state.theme);
    render();
  });
  document.querySelector<HTMLFormElement>("#connection-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    state.apiBase = document.querySelector<HTMLInputElement>("#api-base")?.value.trim() ?? "";
    state.token = document.querySelector<HTMLInputElement>("#token")?.value.trim() ?? "";
    localStorage.setItem("nexo-api-base", state.apiBase); localStorage.setItem("nexo-token", state.token); feedback(text("Connection saved.", "Conexión guardada."));
  });
  document.querySelector<HTMLButtonElement>("#new-case")?.addEventListener("click", async () => {
    const button = document.querySelector<HTMLButtonElement>("#new-case");
    if (!button) return;
    if (!state.token) { feedback(text("Add your workspace token above and save the connection before creating a case.", "Agregá arriba el token de tu espacio de trabajo y guardá la conexión antes de crear un caso."), true); document.querySelector<HTMLInputElement>("#token")?.focus(); return; }
    try { await whileBusy(button, text("Creating…", "Creando…"), async () => { const created = await request<{ case_id: number }>("/v1/cases", { method: "POST" }); state.caseId = created.case_id; localStorage.setItem("nexo-case-id", String(state.caseId)); state.detail = { case_id: state.caseId, nodes: [] }; state.evaluation = null; state.evaluationId = null; state.preparation = null; render(); }); }
    catch (error) { feedback(error instanceof Error ? error.message : text("Could not create the case.", "No se pudo crear el caso."), true); }
  });
  document.querySelector<HTMLFormElement>("#evidence-form")?.addEventListener("submit", async (event) => {
    event.preventDefault(); if (!state.caseId) return feedback(text("Create a case first.", "Primero creá un caso."), true);
    const evidenceText = document.querySelector<HTMLTextAreaElement>("#evidence-text")?.value ?? "";
    const file = document.querySelector<HTMLInputElement>("#evidence-file")?.files?.[0];
    const kind = document.querySelector<HTMLSelectElement>("#evidence-kind")?.value as EvidenceKind;
    if (!file && !evidenceText.trim()) return feedback(text("Choose a file or paste the evidence first.", "Primero elegí un archivo o pegá la evidencia."), true);
    const button = document.querySelector<HTMLButtonElement>("#evidence-form button[type=submit]");
    if (!button) return;
    try { await whileBusy(button, text("Reading…", "Leyendo…"), async () => { const encoded = file ? (kind === "pdf" ? await fileToBase64(file) : await file.text()) : evidenceText; await request(`/v1/cases/${state.caseId}/evidence`, { method: "POST", body: JSON.stringify({ filename: file?.name ?? null, kind, text: encoded }) }); await loadCase(); }); feedback(text("Evidence recorded; its reading is visible in the case timeline.", "Evidencia guardada; su lectura aparece en la línea del caso.")); }
    catch (error) { feedback(error instanceof Error ? error.message : text("Evidence intake failed.", "Falló el ingreso de evidencia."), true); }
  });
  document.querySelector<HTMLButtonElement>("#confirm-assertion")?.addEventListener("click", async () => {
    if (!state.caseId) return feedback(text("Create a case first.", "Primero creá un caso."), true);
    const button = document.querySelector<HTMLButtonElement>("#confirm-assertion");
    if (!button) return;
    try { await whileBusy(button, text("Recording…", "Registrando…"), async () => { await request(`/v1/cases/${state.caseId}/assertions`, { method: "POST", body: JSON.stringify({ confirmed: true }) }); await loadCase(); }); feedback(text("Confirmed assertion recorded.", "Afirmación confirmada registrada.")); }
    catch (error) { feedback(error instanceof Error ? error.message : text("Could not record assertion.", "No se pudo registrar la afirmación."), true); }
  });
  document.querySelector<HTMLButtonElement>("#evaluate")?.addEventListener("click", async () => {
    if (!state.caseId) return feedback(text("Create a case first.", "Primero creá un caso."), true);
    const button = document.querySelector<HTMLButtonElement>("#evaluate");
    if (!button) return;
    try { await whileBusy(button, text("Evaluating…", "Evaluando…"), async () => { const response = await request<{ evaluation_id: number; result: Evaluation }>(`/v1/cases/${state.caseId}/evaluate`, { method: "POST" }); state.evaluationId = response.evaluation_id; state.evaluation = response.result; state.preparation = null; render(); }); }
    catch (error) { feedback(error instanceof Error ? error.message : text("Evaluation failed.", "Falló la evaluación."), true); }
  });
  document.querySelector<HTMLButtonElement>("#prepare")?.addEventListener("click", async () => {
    if (!state.caseId || !state.evaluationId) return feedback(text("Run an available evaluation first.", "Primero ejecutá una evaluación disponible."), true);
    const button = document.querySelector<HTMLButtonElement>("#prepare");
    if (!button) return;
    try {
      await whileBusy(button, text("Preparing…", "Preparando…"), async () => {
        state.preparation = await request<Preparation>(`/v1/cases/${state.caseId}/preparations`, { method: "POST", body: JSON.stringify({ evaluation_id: state.evaluationId, kind: "draft_request" }) });
        render();
      });
      feedback(text("Local preparation is ready; nothing was sent externally.", "La preparación local está lista; no se envió nada externamente."));
    } catch (error) { feedback(error instanceof Error ? error.message : text("Preparation failed.", "Falló la preparación."), true); }
  });
  document.querySelector<HTMLButtonElement>("#export")?.addEventListener("click", async () => {
    if (!state.caseId || !state.preparation) return;
    const button = document.querySelector<HTMLButtonElement>("#export");
    if (!button) return;
    try {
      await whileBusy(button, text("Exporting…", "Exportando…"), async () => {
        state.preparation = await request<Preparation>(`/v1/cases/${state.caseId}/preparations/${state.preparation?.preparation_id}/export`, { method: "POST" });
        render();
      });
      feedback(text("Export verified and available for download; no external act was performed.", "Exportación verificada y disponible para descargar; no se realizó ningún acto externo."));
    } catch (error) { feedback(error instanceof Error ? error.message : text("Export failed verification.", "Falló la verificación de la exportación."), true); }
  });
  document.querySelector<HTMLButtonElement>("#download-manifest")?.addEventListener("click", async () => {
    if (!state.caseId || !state.preparation) return;
    try { downloadBlob(await requestBytes(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export/manifest`), `nexo-preparation-${state.preparation.preparation_id}-manifest.json`); }
    catch (error) { feedback(error instanceof Error ? error.message : text("Manifest download failed.", "Falló la descarga del manifiesto."), true); }
  });
  document.querySelector<HTMLButtonElement>("#download-artifact")?.addEventListener("click", async () => {
    if (!state.caseId || !state.preparation) return;
    try {
      const manifest = await request<{ artifacts: Array<{ digest: string }> }>(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export/manifest`);
      const digest = manifest.artifacts[0]?.digest;
      if (!digest) throw new Error(text("Export has no downloadable artifact.", "La exportación no tiene un artefacto descargable."));
      downloadBlob(await requestBytes(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export/artifacts/${digest}`), `nexo-preparation-${state.preparation.preparation_id}-${digest}.bin`);
    } catch (error) { feedback(error instanceof Error ? error.message : text("Artifact download failed.", "Falló la descarga del artefacto."), true); }
  });
}

if (state.caseId && state.token) {
  state.view = "workspace";
  localStorage.setItem("nexo-view", "workspace");
}
render();
if (state.caseId && state.token) {
  loadCase().catch((error) => feedback(error instanceof Error ? error.message : text("Could not load the saved case.", "No se pudo cargar el caso guardado."), true));
}
