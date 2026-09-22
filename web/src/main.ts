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
  apiBase: localStorage.getItem("nexo-api-base") ?? "",
  token: localStorage.getItem("nexo-token") ?? "",
  caseId: Number(localStorage.getItem("nexo-case-id")) || null,
  detail: null as CaseDetail | null,
  evaluation: null as Evaluation | null,
  evaluationId: null as number | null,
  preparation: null as Preparation | null,
};

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
  if (!state.detail) return `<p class="empty">Create or load a case to see its evidence timeline.</p>`;
  if (state.detail.nodes.length === 0) return `<p class="empty">This case has no graph nodes yet.</p>`;
  return `<ol class="timeline">${state.detail.nodes.map((node) => `
    <li class="node node-${node.kind}">
      <span class="node-kind">${escapeHtml(node.kind.replace("_", " "))}</span>
      <strong>Node ${node.node_id}</strong>
      <time datetime="${escapeHtml(node.created_at)}">${escapeHtml(new Date(node.created_at).toLocaleString())}</time>
      ${node.kind === "user_assertion" ? `<span>${node.confirmed ? "Confirmed by the owner" : "Not confirmed"}</span>` : ""}
    </li>`).join("")}</ol>`;
}

function renderEvidence(): string {
  const supports = state.evaluation?.factual_support ?? state.evaluation?.factual_context ?? [];
  if (!supports.length) return `<p class="muted">No factual support rendered yet.</p>`;
  return `<ul class="support-list">${supports.map((item) => `<li>${escapeHtml(item.kind)} · node ${item.case_node_id ?? "unresolved"}</li>`).join("")}</ul>`;
}

function renderEvaluation(): string {
  const evaluation = state.evaluation;
  if (!evaluation) return `<p class="empty">Run an evaluation to see whether an action is available.</p>`;
  const actionable = evaluation.kind === "actionable";
  const title = actionable ? (evaluation.status === "supported" ? "Supported action" : "Conditionally supported") : `No action: ${evaluation.variant ?? "non-actionable"}`;
  return `<article class="evaluation ${actionable && evaluation.available ? "positive" : "caution"}">
    <div class="evaluation-heading"><span class="eyebrow">${actionable ? "ACTION OPTION" : "HONEST NEGATIVE"}</span><h3>${escapeHtml(title)}</h3></div>
    <p>${actionable ? (evaluation.available ? "This route is available from the recorded support." : "This route is not available until its requirements are met.") : escapeHtml(evaluation.cause ? `Reason: ${evaluation.cause.replaceAll("_", " ")}.` : "The current case does not support an available action.")}</p>
    <h4>Why this is shown</h4>${renderEvidence()}
    ${evaluation.legal_support?.length ? `<h4>Legal support</h4><ul class="citation-list">${evaluation.legal_support.map((citation) => { const sourceUrl = safeExternalUrl(citation.source_locator); return `<li><strong>${escapeHtml(citation.proposition)}</strong><span>${escapeHtml(citation.source_issuer)}</span>${sourceUrl ? `<a href="${escapeHtml(sourceUrl)}" target="_blank" rel="noreferrer">Open captured source</a>` : "<span class=\"muted\">Captured source has no safe web URL</span>"}</li>`; }).join("")}</ul>` : ""}
  </article>`;
}

function renderPreparation(): string {
  const evaluation = state.evaluation;
  if (!evaluation?.available || !state.evaluationId) {
    return `<p class="muted">A preparation appears only after the current evaluation returns an available action.</p>`;
  }
  const preparation = state.preparation;
  return `<div class="preparation-card">
    <p class="muted">This creates local, deterministic material. It does not send, file, sign, or notify anyone.</p>
    ${preparation ? `<p><strong>Preparation ${preparation.preparation_id}</strong> · <span class="badge">${escapeHtml(preparation.status)}</span>${preparation.manifest_digest ? `<br><span class="digest">Manifest ${escapeHtml(preparation.manifest_digest)}</span>` : ""}</p>` : "<p class=\"muted\">No draft prepared for this evaluation yet.</p>"}
    <div class="button-row">
      <button id="prepare" class="secondary">${preparation ? "Refresh draft" : "Prepare draft"}</button>
      ${preparation ? `<button id="export"${preparation.status === "exported" ? " class=\"secondary\"" : ""}>${preparation.status === "exported" ? "Verify export" : "Export locally"}</button>` : ""}
      ${preparation?.status === "exported" ? `<button id="download-manifest" class="secondary">Download manifest</button><button id="download-artifact" class="secondary">Download artifact</button>` : ""}
    </div>
  </div>`;
}

function render(): void {
  app.innerHTML = `<main class="shell">
    <header class="topbar"><div><span class="eyebrow">VERIFIABLE DIGITAL-RIGHTS CASE GRAPH</span><h1>NEXO</h1></div><span class="status-dot">Local workspace</span></header>
    <section class="setup panel"><div><h2>Connect your workspace</h2><p class="muted">NEXO renders evidence and evaluation results. It does not file or send anything externally.</p></div><form id="connection-form" class="connection-form">
      <label>API base <input id="api-base" value="${escapeHtml(state.apiBase)}" placeholder="Same origin or http://127.0.0.1:8080" /></label>
      <label>Bearer token <input id="token" type="password" value="${escapeHtml(state.token)}" autocomplete="off" required /></label>
      <button type="submit">Save connection</button>
    </form></section>
    <section class="workspace-grid">
      <div class="column"><section class="panel"><div class="section-heading"><div><span class="eyebrow">CASE</span><h2>${state.caseId ? `Case ${state.caseId}` : "Start a case"}</h2></div><button id="new-case" class="secondary">New case</button></div><div id="case-feedback" class="feedback" aria-live="polite"></div>${renderTimeline()}</section>
      <section class="panel"><span class="eyebrow">EVIDENCE INTAKE</span><h2>Add what happened</h2><form id="evidence-form"><label>Filename <input id="filename" placeholder="message.txt" /></label><label>Plain-text evidence <textarea id="evidence-text" rows="7" placeholder="Paste the relevant evidence here"></textarea></label><button type="submit">Send to sandbox</button></form><p class="muted small">The API stores the original bytes and runs extraction in the sandbox. Rejections remain visible.</p></section></div>
      <div class="column"><section class="panel"><span class="eyebrow">OWNER ASSERTION</span><h2>Confirm a fact</h2><p class="muted">A confirmation is a user assertion, not an extracted observation.</p><button id="confirm-assertion" class="secondary">Record confirmed assertion</button></section>
      <section class="panel"><div class="section-heading"><div><span class="eyebrow">EVALUATION</span><h2>Why is this shown?</h2></div><button id="evaluate">Evaluate case</button></div><div id="evaluation-output">${renderEvaluation()}</div><div class="preparation-section"><span class="eyebrow">PREPARATION / EXPORT</span><h3>Keep the proof portable</h3>${renderPreparation()}</div></section></div>
    </section><footer><span>NEXO stops at preparation. A human remains the actor for any external legal act.</span></footer>
  </main>`;
  bindEvents();
}

function feedback(message: string, error = false): void {
  const target = document.querySelector<HTMLDivElement>("#case-feedback");
  if (target) { target.textContent = message; target.className = `feedback ${error ? "error" : "success"}`; }
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
  document.querySelector<HTMLFormElement>("#connection-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    state.apiBase = document.querySelector<HTMLInputElement>("#api-base")?.value.trim() ?? "";
    state.token = document.querySelector<HTMLInputElement>("#token")?.value.trim() ?? "";
    localStorage.setItem("nexo-api-base", state.apiBase); localStorage.setItem("nexo-token", state.token); feedback("Connection saved.");
  });
  document.querySelector<HTMLButtonElement>("#new-case")?.addEventListener("click", async () => {
    try { const created = await request<{ case_id: number }>("/v1/cases", { method: "POST" }); state.caseId = created.case_id; localStorage.setItem("nexo-case-id", String(state.caseId)); state.detail = { case_id: state.caseId, nodes: [] }; state.evaluation = null; state.evaluationId = null; state.preparation = null; render(); }
    catch (error) { feedback(error instanceof Error ? error.message : "Could not create the case.", true); }
  });
  document.querySelector<HTMLFormElement>("#evidence-form")?.addEventListener("submit", async (event) => {
    event.preventDefault(); if (!state.caseId) return feedback("Create a case first.", true);
    const text = document.querySelector<HTMLTextAreaElement>("#evidence-text")?.value ?? "";
    try { await request(`/v1/cases/${state.caseId}/evidence`, { method: "POST", body: JSON.stringify({ filename: document.querySelector<HTMLInputElement>("#filename")?.value || null, text }) }); await loadCase(); feedback("Evidence recorded; extraction result is visible in the case timeline."); }
    catch (error) { feedback(error instanceof Error ? error.message : "Evidence intake failed.", true); }
  });
  document.querySelector<HTMLButtonElement>("#confirm-assertion")?.addEventListener("click", async () => {
    if (!state.caseId) return feedback("Create a case first.", true);
    try { await request(`/v1/cases/${state.caseId}/assertions`, { method: "POST", body: JSON.stringify({ confirmed: true }) }); await loadCase(); feedback("Confirmed assertion recorded."); }
    catch (error) { feedback(error instanceof Error ? error.message : "Could not record assertion.", true); }
  });
  document.querySelector<HTMLButtonElement>("#evaluate")?.addEventListener("click", async () => {
    if (!state.caseId) return feedback("Create a case first.", true);
    try { const response = await request<{ evaluation_id: number; result: Evaluation }>(`/v1/cases/${state.caseId}/evaluate`, { method: "POST" }); state.evaluationId = response.evaluation_id; state.evaluation = response.result; state.preparation = null; render(); }
    catch (error) { feedback(error instanceof Error ? error.message : "Evaluation failed.", true); }
  });
  document.querySelector<HTMLButtonElement>("#prepare")?.addEventListener("click", async () => {
    if (!state.caseId || !state.evaluationId) return feedback("Run an available evaluation first.", true);
    try {
      state.preparation = await request<Preparation>(`/v1/cases/${state.caseId}/preparations`, { method: "POST", body: JSON.stringify({ evaluation_id: state.evaluationId, kind: "draft_request" }) });
      render();
      feedback("Local preparation is ready; nothing was sent externally.");
    } catch (error) { feedback(error instanceof Error ? error.message : "Preparation failed.", true); }
  });
  document.querySelector<HTMLButtonElement>("#export")?.addEventListener("click", async () => {
    if (!state.caseId || !state.preparation) return;
    try {
      state.preparation = await request<Preparation>(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export`, { method: "POST" });
      render();
      feedback("Export verified and available for download; no external act was performed.");
    } catch (error) { feedback(error instanceof Error ? error.message : "Export failed verification.", true); }
  });
  document.querySelector<HTMLButtonElement>("#download-manifest")?.addEventListener("click", async () => {
    if (!state.caseId || !state.preparation) return;
    try { downloadBlob(await requestBytes(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export/manifest`), `nexo-preparation-${state.preparation.preparation_id}-manifest.json`); }
    catch (error) { feedback(error instanceof Error ? error.message : "Manifest download failed.", true); }
  });
  document.querySelector<HTMLButtonElement>("#download-artifact")?.addEventListener("click", async () => {
    if (!state.caseId || !state.preparation) return;
    try {
      const manifest = await request<{ artifacts: Array<{ digest: string }> }>(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export/manifest`);
      const digest = manifest.artifacts[0]?.digest;
      if (!digest) throw new Error("Export has no downloadable artifact.");
      downloadBlob(await requestBytes(`/v1/cases/${state.caseId}/preparations/${state.preparation.preparation_id}/export/artifacts/${digest}`), `nexo-preparation-${state.preparation.preparation_id}-${digest}.bin`);
    } catch (error) { feedback(error instanceof Error ? error.message : "Artifact download failed.", true); }
  });
}

render();
if (state.caseId && state.token) {
  loadCase().catch((error) => feedback(error instanceof Error ? error.message : "Could not load the saved case.", true));
}
