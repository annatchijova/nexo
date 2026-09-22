import "./styles.css";

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

const state = {
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
  if (!response.ok) throw new Error(`${response.status}: ${(await response.text()) || response.statusText}`);
  return response.json() as Promise<T>;
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

function render(): void {
  applyTheme();
  app.innerHTML = `<main class="shell">
    <header class="topbar"><div><span class="eyebrow">${text("VERIFIABLE DIGITAL-RIGHTS CASE GRAPH", "GRAFO VERIFICABLE DE DERECHOS DIGITALES")}</span><h1>NEXO</h1></div><div class="topbar-actions"><span class="status-dot">${text("Local workspace", "Espacio local")}</span><button id="language-toggle" class="utility-button" type="button">${state.language === "en" ? "ES" : "EN"}</button><button id="theme-toggle" class="utility-button" type="button">${state.theme === "dark" ? text("Light mode", "Modo claro") : text("Dark mode", "Modo oscuro")}</button></div></header>
    <section class="setup panel"><div><h2>${text("Connect your workspace", "Conectá tu espacio de trabajo")}</h2><p class="muted">${text("NEXO renders evidence and evaluation results. It does not file or send anything externally.", "NEXO muestra evidencia y resultados de evaluación. No presenta ni envía nada externamente.")}</p></div><form id="connection-form" class="connection-form">
      <label>${text("API base", "Base de API")} <input id="api-base" value="${escapeHtml(state.apiBase)}" placeholder="Same origin or http://127.0.0.1:8080" /></label>
      <label>${text("Bearer token", "Token bearer")} <input id="token" type="password" value="${escapeHtml(state.token)}" autocomplete="off" required /></label>
      <button type="submit">${text("Save connection", "Guardar conexión")}</button>
    </form></section>
    <section class="workspace-grid">
      <div class="column"><section class="panel"><div class="section-heading"><div><span class="eyebrow">${text("CASE", "CASO")}</span><h2>${state.caseId ? `${text("Case", "Caso")} ${state.caseId}` : text("Start a case", "Iniciar un caso")}</h2></div><button id="new-case" class="secondary">${text("New case", "Nuevo caso")}</button></div><div id="case-feedback" class="feedback" aria-live="polite"></div>${renderTimeline()}</section>
      <section class="panel"><span class="eyebrow">${text("EVIDENCE INTAKE", "INGRESO DE EVIDENCIA")}</span><h2>${text("Add what happened", "Agregá lo que pasó")}</h2><form id="evidence-form"><label>${text("Filename", "Nombre de archivo")} <input id="filename" placeholder="message.txt" /></label><label>${text("Plain-text evidence", "Evidencia en texto plano")} <textarea id="evidence-text" rows="7" placeholder="${text("Paste the relevant evidence here", "Pegá aquí la evidencia relevante")}"></textarea></label><button type="submit">${text("Send to sandbox", "Enviar al sandbox")}</button></form><p class="muted small">${text("The API stores the original bytes and runs extraction in the sandbox. Rejections remain visible.", "La API guarda los bytes originales y extrae la información en el sandbox. Las rechazos siguen visibles.")}</p></section></div>
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
    try { await whileBusy(button, text("Creating…", "Creando…"), async () => { const created = await request<{ case_id: number }>("/v1/cases", { method: "POST" }); state.caseId = created.case_id; localStorage.setItem("nexo-case-id", String(state.caseId)); state.detail = { case_id: state.caseId, nodes: [] }; state.evaluation = null; state.evaluationId = null; state.preparation = null; render(); }); }
    catch (error) { feedback(error instanceof Error ? error.message : text("Could not create the case.", "No se pudo crear el caso."), true); }
  });
  document.querySelector<HTMLFormElement>("#evidence-form")?.addEventListener("submit", async (event) => {
    event.preventDefault(); if (!state.caseId) return feedback(text("Create a case first.", "Primero creá un caso."), true);
    const evidenceText = document.querySelector<HTMLTextAreaElement>("#evidence-text")?.value ?? "";
    const button = document.querySelector<HTMLButtonElement>("#evidence-form button[type=submit]");
    if (!button) return;
    try { await whileBusy(button, text("Extracting…", "Extrayendo…"), async () => { await request(`/v1/cases/${state.caseId}/evidence`, { method: "POST", body: JSON.stringify({ filename: document.querySelector<HTMLInputElement>("#filename")?.value || null, text: evidenceText }) }); await loadCase(); }); feedback(text("Evidence recorded; extraction result is visible in the case timeline.", "Evidencia registrada; el resultado de extracción aparece en la línea del caso.")); }
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

render();
if (state.caseId && state.token) {
  loadCase().catch((error) => feedback(error instanceof Error ? error.message : text("Could not load the saved case.", "No se pudo cargar el caso guardado."), true));
}
