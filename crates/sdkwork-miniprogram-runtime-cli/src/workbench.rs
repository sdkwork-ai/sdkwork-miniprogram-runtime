//! Embedded browser workbench UI for local mini-program preview and source editing.

/// Generates the workbench shell. The local Rust host serves the API endpoints from the same origin.
#[must_use]
pub fn generate_html(project_name: &str) -> String {
    let mut html = String::with_capacity(24_000);
    html.push_str(r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light"><title>"#);
    escape_html(&mut html, project_name);
    html.push_str(r" · Mini Program Workbench</title><style>");
    html.push_str(STYLESHEET);
    html.push_str(".inspector-toggle.active{background:var(--blue-soft);border-color:#d8e3ff;color:#2857cd}.inspector-panel.hidden{display:none}.inspector-console-row{display:grid;grid-template-columns:74px 52px minmax(0,1fr);gap:8px;padding:4px 0;border-bottom:1px solid #edf0f4}.inspector-console-row time{color:#8994a3}.inspector-console-row strong{font-size:9px}.inspector-console-row.level-error strong{color:#bc3d3d}.inspector-console-row.level-warn strong{color:#a4771b}.inspector-console-row.level-info strong{color:#2e6cb5}.inspector-console-row span{white-space:pre-wrap;overflow-wrap:anywhere}.debug-toolbar{display:flex;gap:6px;margin-bottom:8px;padding-bottom:6px;border-bottom:1px solid #edf0f4}.debug-toolbar select,.debug-toolbar button{height:25px;border:1px solid #dce2ea;border-radius:4px;background:#fff;color:#45546a;font:10px inherit}.debug-toolbar button{padding:0 8px;cursor:pointer}.network-row{display:block;width:100%;padding:7px;border:0;border-bottom:1px solid #edf0f4;background:#fff;text-align:left;font:inherit;cursor:pointer}.network-row:hover{background:#f5f7fa}.network-details{white-space:pre-wrap;overflow-wrap:anywhere}");
    html.push_str(r#"</style></head><body>
<div class="app-shell">
  <header class="topbar">
    <div class="brand"><span class="brand-mark">S</span><div><strong>SDKWORK</strong><span>MINI PROGRAM STUDIO</span></div></div>
    <div class="project-title"><span class="crumb">Workspace</span><span class="slash">/</span><strong id="project-name">"#);
    escape_html(&mut html, project_name);
    html.push_str(r#"</strong><span class="save-state" id="save-state">Ready</span><button class="switch-project" id="switch-project">Switch project</button></div>
    <div class="top-actions"><button class="icon-button" id="reload-preview" title="Reload preview" aria-label="Reload preview">↻</button><button class="icon-button inspector-toggle active" id="toggle-inspector" title="Hide Inspector" aria-label="Hide Inspector" aria-pressed="true">▤</button><button class="primary-button" id="save-file">Save</button></div>
  </header>
  <main class="workspace">
    <section class="preview-panel">
      <div class="panel-toolbar">
        <div class="panel-heading"><span class="live-dot"></span><strong>Preview</strong></div>
        <select class="page-switch" id="page-select" aria-label="Preview page"></select>
        <div class="device-switch" role="group" aria-label="Preview device">
          <button class="device-option active" data-device="phone" aria-pressed="true">Phone</button>
          <button class="device-option" data-device="tablet" aria-pressed="false">Tablet</button>
          <button class="device-option" data-device="pc" aria-pressed="false">PC</button>
        </div>
        <button class="icon-button" id="fullscreen-preview" title="Open preview in new tab">↗</button>
      </div>
      <div class="preview-stage" id="preview-stage" data-device="phone">
        <div class="device-frame" id="device-frame"><div class="device-screen"><iframe id="preview-frame" title="Mini program preview" sandbox="allow-scripts allow-same-origin"></iframe></div></div>
      </div>
      <div class="preview-footer"><span id="viewport-label">390 × 844 · Phone</span><span class="readonly-note"><span class="lock-icon">▣</span> Runtime preview</span></div>
    </section>
    <section class="code-panel">
      <div class="explorer">
        <div class="explorer-toolbar"><span>EXPLORER</span><button class="icon-button small" id="refresh-tree" title="Refresh files">↻</button></div>
        <div class="project-root"><span class="chevron">⌄</span><span class="folder-icon">▰</span><strong id="tree-root">"#);
    escape_html(&mut html, project_name);
    html.push_str(r#"</strong></div>
        <nav class="file-tree" id="file-tree" aria-label="Project files"><div class="tree-loading">Loading project files…</div></nav>
        <div class="explorer-bottom"><span class="status-dot"></span><span>Local workspace</span></div>
      </div>
      <div class="editor-panel">
        <div class="editor-tabs"><div class="active-tab" id="active-tab"><span class="file-glyph">JS</span><span id="active-file-name">Select a file</span><span class="dirty-indicator" id="dirty-indicator"></span></div><div class="editor-tab-actions"><span class="editor-language" id="language-label">Ready</span><button class="icon-button small" id="format-help" title="Editor shortcuts">⌘</button></div></div>
        <div class="editor-body"><div id="editor" aria-label="Source editor"></div><div class="editor-loading" id="editor-loading"><div class="loader-mark">{ }</div><strong>Preparing editor</strong><span>Loading Monaco Editor</span></div><div class="editor-empty hidden" id="editor-empty"><div class="empty-icon">⌘</div><strong>Select a project file</strong><span>Choose a file in Explorer to inspect and edit its source.</span></div></div>
        <div class="statusbar"><span id="editor-status">SDKWORK Mini Program Runtime</span><span class="statusbar-right"><span id="cursor-position">Ln 1, Col 1</span><span id="encoding-label">UTF-8</span><span id="file-language">Plain Text</span></span></div>
      </div>
    </section>
  </main>
  <section class="inspector-panel" id="inspector-panel">
    <div class="inspector-toolbar"><strong>Inspector</strong><span class="inspector-status" id="inspector-status">Select a rendered element</span><div class="inspector-tabs"><button class="inspector-tab active" data-inspector-tab="tree">Elements</button><button class="inspector-tab" data-inspector-tab="styles">Styles</button><button class="inspector-tab" data-inspector-tab="data">Data</button><button class="inspector-tab" data-inspector-tab="console">Console</button><button class="inspector-tab" data-inspector-tab="network">Network</button><button class="inspector-tab" data-inspector-tab="diagnostics">Diagnostics</button><button class="inspector-tab" data-inspector-tab="target">Target</button></div><button class="icon-button small inspector-close" id="close-inspector" title="Hide Inspector" aria-label="Hide Inspector">×</button></div>
    <div class="inspector-content" id="inspector-content"></div>
  </section>
  <dialog class="project-dialog" id="project-dialog"><form method="dialog"><header><strong>Switch project</strong><button class="icon-button small" value="cancel" aria-label="Close">×</button></header><label for="project-parent">Search folder</label><div class="project-path-row"><input id="project-parent" type="text" autocomplete="off" spellcheck="false" placeholder="Paste the folder containing projects"><button class="secondary-button" id="browse-projects">Browse</button></div><div class="project-list" id="project-list"></div><label for="project-path">Project folder</label><div class="project-path-row"><input id="project-path" type="text" autocomplete="off" spellcheck="false" placeholder="Select a project or paste its folder path"><button class="primary-button" id="load-project">Open</button></div><p class="project-error" id="project-error"></p></form></dialog>
  <div class="toast" id="toast" role="status"></div>
</div>
<script src="https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs/loader.js"></script>
<script>"#);
    html.push_str(SCRIPT);
    html.push_str(r"</script></body></html>");
    html
}

fn escape_html(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
}

const STYLESHEET: &str = r"
";

const SCRIPT: &str = r"
const $ = (id) => document.getElementById(id);
const state = {
  device: 'phone',
  tree: [],
  activePath: null,
  editor: null,
  dirty: false,
  pages: [],
  page: null,
  pendingFile: '',
  inspection: null,
  selectedNode: null,
    inspectorTab: 'tree',
  inspectorVisible: true,
  consoleFilter: 'all',
  networkFilter: '',
};
const devices = {
  phone: { width: 390, height: 844, label: 'Phone' },
  tablet: { width: 768, height: 1024, label: 'Tablet' },
  pc: { width: 1440, height: 900, label: 'PC' },
};
function toast(message) {
  const element = $('toast');
  element.textContent = message;
  element.classList.add('visible');
  setTimeout(() => element.classList.remove('visible'), 2200);
}
function setDirty(value) {
  state.dirty = value;
  $('dirty-indicator').classList.toggle('visible', value);
  $('save-state').textContent = value ? 'Unsaved changes' : 'All changes saved';
  $('save-state').classList.toggle('saved', !value);
}
function query(path, params = {}) {
  const url = new URL(path, location.origin);
  for (const [key, value] of Object.entries(params)) url.searchParams.set(key, value);
  return url;
}
async function api(path, options = {}) {
  const response = await fetch(path, {
    cache: 'no-store',
    ...options,
    headers: {
      ...(options.body ? { 'Content-Type': 'application/json' } : {}),
      ...(options.headers || {}),
    },
  });
  if (!response.ok) throw new Error((await response.text()) || `Request failed (${response.status})`);
  return response.json();
}
function iconFor(path) {
  const extension = path.split('.').pop().toLowerCase();
  return ({ js: 'JS', ts: 'TS', rs: 'RS', json: '{}', wxml: 'W', wxss: '#', css: '#', md: 'M', html: 'H' })[extension] || '·';
}
function languageFor(path) {
  const extension = path.split('.').pop().toLowerCase();
  return ({ js: 'javascript', ts: 'typescript', rs: 'rust', json: 'json', wxml: 'html', wxss: 'css', css: 'css', md: 'markdown' })[extension] || 'plaintext';
}
function renderTree(entries, container, depth = 0) {
  for (const node of entries) {
    const directory = node.kind === 'directory';
    const row = document.createElement('div');
    row.className = `tree-row ${directory ? 'directory' : 'file'}`;
    row.style.paddingLeft = `${8 + depth * 14}px`;
    const chevron = document.createElement('span');
    chevron.className = 'tree-chevron';
    chevron.textContent = directory ? '›' : '';
    const icon = document.createElement('span');
    icon.className = 'tree-icon';
    icon.textContent = directory ? '▰' : iconFor(node.path || node.name || '');
    const label = document.createElement('span');
    label.className = 'tree-label';
    label.textContent = node.name;
    row.append(chevron, icon, label);
    row.dataset.depth = depth;
    if (directory) {
      row.addEventListener('click', () => {
        const expanded = row.dataset.open === 'true';
        row.dataset.open = String(!expanded);
        chevron.textContent = expanded ? '›' : '⌄';
        let next = row.nextElementSibling;
        while (next && Number(next.dataset.depth) > depth) {
          const remove = next;
          next = next.nextElementSibling;
          remove.remove();
        }
        if (!expanded) {
          const fragment = document.createDocumentFragment();
          renderTree(node.children || [], fragment, depth + 1);
          row.after(fragment);
        }
      });
    } else {
      row.dataset.path = node.path;
      row.addEventListener('click', () => openFile(node.path, row));
    }
    container.append(row);
    if (directory && ['pages', 'components'].includes(node.name)) {
      row.dataset.open = 'true';
      chevron.textContent = '⌄';
      renderTree(node.children || [], container, depth + 1);
    }
  }
}
async function loadTree() {
  const target = $('file-tree');
  target.replaceChildren();
  try {
    const result = await api('/api/tree');
    $('tree-root').textContent = result.rootName || 'PROJECT';
    $('project-name').textContent = result.rootName || 'PROJECT';
    state.tree = result.entries || [];
    renderTree(state.tree, target);
  } catch (error) {
    const message = document.createElement('div');
    message.className = 'tree-loading';
    message.textContent = `Could not load project tree: ${error.message}`;
    target.append(message);
  }
}
async function loadPages() {
  const result = await api('/api/pages');
  state.pages = result.pages || [];
  state.page = result.entryPage || state.pages[0] || null;
  const select = $('page-select');
  select.replaceChildren();
  for (const route of state.pages) {
    const option = document.createElement('option');
    option.value = route;
    option.textContent = route;
    select.append(option);
  }
  select.value = state.page || '';
}
async function loadInspection() {
  if (!state.page) return;
  try {
    state.inspection = await api(query('/api/inspect', { page: state.page }));
    state.selectedNode = null;
    renderInspector();
  } catch (error) {
    state.inspection = null;
    $('inspector-status').textContent = `Inspector unavailable: ${error.message}`;
    $('inspector-content').replaceChildren();
  }
}
function flattenNodes(nodes, depth = 0, result = []) {
  for (const node of nodes || []) {
    result.push({ node, depth });
    flattenNodes(node.children, depth + 1, result);
  }
  return result;
}
function renderInspector() {
  const target = $('inspector-content');
  target.replaceChildren();
  const data = state.inspection;
  if (!data) {
    $('inspector-status').textContent = 'No inspection data';
    return;
  }
  const selected = state.selectedNode;
  const flat = flattenNodes(data.renderTree?.children);
  const selectedEntry = flat.find(({ node }) => Number(node.id) === Number(selected));
  $('inspector-status').textContent = selectedEntry
    ? `${selectedEntry.node.tag_name || selectedEntry.node.kind} · #${selected}`
    : `${data.route} · ${flat.length} nodes`;
  if (state.inspectorTab === 'tree') {
    for (const { node, depth } of flat) {
      const button = document.createElement('button');
      button.className = `inspector-tree-row${Number(node.id) === Number(selected) ? ' selected' : ''}`;
      button.style.paddingLeft = `${7 + depth * 16}px`;
      button.textContent = `${node.tag_name || node.kind}${node.id !== undefined ? `  #${node.id}` : ''}`;
      button.addEventListener('click', () => selectNode(node.id));
      target.append(button);
    }
    if (!flat.length) target.textContent = 'This page has no render nodes.';
    return;
  }
  if (state.inspectorTab === 'styles') {
    if (!selectedEntry) {
      target.textContent = 'Select an element in the preview or Elements tree.';
      return;
    }
    const styles = data.computedStyles?.[String(selected)] || {};
    target.textContent = Object.keys(styles).length
      ? Object.entries(styles).map(([name, value]) => `${name}: ${value};`).join('\n')
      : 'No supported declarations matched this element.';
    const source = selectedEntry.node.source;
    if (source) target.textContent += `\n\nSource\n${state.page}.wxml:${source.line}:${source.column}`;
    return;
  }
  if (state.inspectorTab === 'data') {
    target.textContent = JSON.stringify(data.pageData || {}, null, 2);
    return;
  }
  if (state.inspectorTab === 'console') {
    const entries = Array.isArray(data.console) ? data.console : [];
    target.replaceChildren();
    const toolbar = document.createElement('div');
    toolbar.className = 'debug-toolbar';
    const filter = document.createElement('select');
    filter.setAttribute('aria-label', 'Filter console level');
    for (const level of ['all', 'log', 'info', 'warn', 'error', 'debug']) {
      const option = document.createElement('option');
      option.value = level;
      option.textContent = level === 'all' ? 'All levels' : level;
      filter.append(option);
    }
    filter.value = state.consoleFilter;
    filter.addEventListener('change', () => { state.consoleFilter = filter.value; renderInspector(); });
    const clear = document.createElement('button');
    clear.type = 'button';
    clear.textContent = 'Clear view';
    clear.addEventListener('click', () => { state.inspection.console = []; renderInspector(); });
    const exportButton = document.createElement('button');
    exportButton.type = 'button';
    exportButton.textContent = 'Export';
    exportButton.addEventListener('click', () => {
      const blob = new Blob([JSON.stringify(entries, null, 2)], { type: 'application/json' });
      const link = document.createElement('a');
      link.href = URL.createObjectURL(blob);
      link.download = 'runtime-console.json';
      link.click();
      URL.revokeObjectURL(link.href);
    });
    toolbar.append(filter, clear, exportButton);
    target.append(toolbar);
    const visibleEntries = entries.filter(entry => state.consoleFilter === 'all' || entry.level === state.consoleFilter);
    if (!visibleEntries.length) {
      const empty = document.createElement('div');
      empty.textContent = entries.length ? 'No records match this filter.' : 'No console records were emitted by this Runtime target.';
      target.append(empty);
      return;
    }
    for (const entry of visibleEntries) {
      const row = document.createElement('div');
      row.className = `inspector-console-row level-${entry.level || 'log'}`;
      const time = document.createElement('time');
      time.textContent = entry.timestamp ? new Date(entry.timestamp).toLocaleTimeString() : '';
      const level = document.createElement('strong');
      level.textContent = String(entry.level || 'log').toUpperCase();
      const message = document.createElement('span');
      message.textContent = (entry.args || []).map(value => typeof value === 'string' ? value : JSON.stringify(value)).join(' ');
      row.append(time, level, message);
      target.append(row);
    }
    return;
  }
  if (state.inspectorTab === 'network') {
    const entries = Array.isArray(data.network) ? data.network : [];
    target.replaceChildren();
    const toolbar = document.createElement('div');
    toolbar.className = 'debug-toolbar';
    const filter = document.createElement('input');
    filter.type = 'search';
    filter.setAttribute('aria-label', 'Filter network requests');
    filter.placeholder = 'Filter requests';
    filter.value = state.networkFilter;
    filter.addEventListener('change', () => { state.networkFilter = filter.value; renderInspector(); });
    const exportButton = document.createElement('button');
    exportButton.type = 'button';
    exportButton.textContent = 'Export';
    exportButton.addEventListener('click', () => {
      const blob = new Blob([JSON.stringify(entries, null, 2)], { type: 'application/json' });
      const link = document.createElement('a');
      link.href = URL.createObjectURL(blob);
      link.download = 'runtime-network.json';
      link.click();
      URL.revokeObjectURL(link.href);
    });
    toolbar.append(filter, exportButton);
    target.append(toolbar);
    if (!entries.length) {
      const empty = document.createElement('div');
      empty.textContent = 'No network records. Network events will appear when the Runtime wx.request provider is enabled.';
      target.append(empty);
      return;
    }
    const filteredEntries = entries.filter(entry => `${entry.method || ''} ${entry.url || ''} ${entry.status || ''}`.toLowerCase().includes(state.networkFilter.toLowerCase()));
    for (const entry of filteredEntries) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'network-row';
      row.textContent = `${entry.method || 'REQUEST'}  ${entry.url || 'unknown URL'}  ${entry.status || ''}  ${entry.durationMs ?? ''} ms`;
      row.addEventListener('click', () => {
        const detail = document.createElement('pre');
        detail.className = 'network-details';
        detail.textContent = JSON.stringify({
          url: entry.url,
          method: entry.method,
          requestHeaders: entry.requestHeaders,
          requestBody: entry.requestBody,
          responseHeaders: entry.responseHeaders,
          status: entry.status,
          durationMs: entry.durationMs,
          failure: entry.failure,
          initiator: entry.initiator,
        }, null, 2);
        target.replaceChildren(row, detail);
      });
      target.append(row);
    }
    if (!filteredEntries.length) target.append('No requests match this filter.');
    return;
  }
  if (state.inspectorTab === 'diagnostics') {
    const diagnostics = data.diagnostics || [];
    target.textContent = diagnostics.length
      ? diagnostics.map(item => `${item.kind || 'style'} ${item.span?.line || ''}:${item.span?.column || ''} ${item.message || ''}`).join('\n')
      : 'No renderer-subset diagnostics.';
    return;
  }
  if (state.inspectorTab === 'target') {
    target.textContent = JSON.stringify({
      targetId: data.target?.targetId || 'unknown',
      type: data.target?.type || 'mini-program',
      state: data.target?.state || 'unknown',
      protocolVersion: data.protocolVersion,
      capabilities: data.debugCapabilities,
    }, null, 2);
    return;
  }
  target.textContent = 'JavaScript runtime debugging is not connected to this workbench yet. The Runtime owns the Inspector session, while transport, pause loop and source mapping are still pending.';
}
function selectNode(id) {
  state.selectedNode = id;
  renderInspector();
  $('preview-frame').contentWindow?.postMessage({ source: 'sdkwork-workbench', type: 'select-node', nodeId: Number(id) }, location.origin);
}
function setInspectorVisible(visible) {
  state.inspectorVisible = visible;
  $('inspector-panel').classList.toggle('hidden', !visible);
  const toggle = $('toggle-inspector');
  toggle.classList.toggle('active', visible);
  toggle.setAttribute('aria-pressed', String(visible));
  toggle.setAttribute('aria-label', visible ? 'Hide Inspector' : 'Show Inspector');
  toggle.title = visible ? 'Hide Inspector' : 'Show Inspector';
}
async function openFile(path, row) {
  if (state.dirty && !confirm('Discard unsaved changes and open another file?')) return;
  try {
    const file = await api(query('/api/file', { path }));
    state.activePath = file.path || path;
    state.pendingFile = file.content || '';
    $('active-file-name').textContent = state.activePath.split('/').pop();
    $('language-label').textContent = languageFor(state.activePath).toUpperCase();
    $('file-language').textContent = languageFor(state.activePath).toUpperCase();
    $('editor-status').textContent = state.activePath;
    document.querySelectorAll('.tree-row.selected').forEach(node => node.classList.remove('selected'));
    row?.classList.add('selected');
    $('editor-empty').classList.add('hidden');
    if (state.editor) {
      const oldModel = state.editor.getModel();
      const model = monaco.editor.createModel(state.pendingFile, languageFor(state.activePath), monaco.Uri.from({ scheme: 'file', path: `/${state.activePath}` }));
      state.editor.setModel(model);
      oldModel?.dispose();
    }
    setDirty(false);
  } catch (error) { toast(`Could not open file: ${error.message}`); }
}
async function saveFile() {
  if (!state.editor || !state.activePath) return;
  try {
    await api('/api/file', { method: 'PUT', body: JSON.stringify({ path: state.activePath, content: state.editor.getValue() }) });
    setDirty(false);
    toast('File saved');
    reloadPreview();
  } catch (error) { toast(`Save failed: ${error.message}`); }
}
function reloadPreview() {
  if (!state.page) return;
  const url = query('/api/preview', { device: state.device, page: state.page });
  url.searchParams.set('revision', String(Date.now()));
  $('preview-frame').src = url.toString();
  loadInspection();
}
function setDevice(device) {
  state.device = device;
  const dimensions = devices[device];
  $('preview-stage').dataset.device = device;
  $('viewport-label').textContent = `${dimensions.width} × ${dimensions.height} · ${dimensions.label}`;
  document.querySelectorAll('.device-option').forEach(button => {
    const active = button.dataset.device === device;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  requestAnimationFrame(() => {
    const stage = $('preview-stage');
    const frame = $('device-frame');
    const outerWidth = dimensions.width + (device === 'pc' ? 0 : 24);
    const outerHeight = dimensions.height + (device === 'pc' ? 0 : 24);
    const scale = Math.min((stage.clientWidth - 48) / outerWidth, (stage.clientHeight - 48) / outerHeight, 1);
    frame.style.transform = `scale(${Math.max(scale, 0.2)})`;
  });
  reloadPreview();
}
async function loadProjectChoices() {
  const parent = $('project-parent').value.trim();
  try {
    const result = await api(query('/api/projects', { parent }));
    const list = $('project-list');
    list.replaceChildren();
    for (const project of result.projects || []) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = `${project.name} · ${project.path}`;
      button.addEventListener('click', () => { $('project-path').value = project.path; });
      list.append(button);
    }
    if (!result.projects?.length) list.textContent = 'No mini-program projects found in this folder.';
  } catch (error) { $('project-error').textContent = error.message; }
}
async function switchProject() {
  const path = $('project-path').value.trim();
  if (!path) return;
  $('project-error').textContent = '';
  try {
    const result = await api('/api/project', { method: 'PUT', body: JSON.stringify({ path }) });
    $('project-dialog').close();
    $('project-name').textContent = result.rootName;
    $('tree-root').textContent = result.rootName;
    state.activePath = null;
    state.inspection = null;
    state.selectedNode = null;
    if (state.editor) state.editor.setValue('');
    $('active-file-name').textContent = 'Select a file';
    setDirty(false);
    await loadTree();
    await loadPages();
    reloadPreview();
  } catch (error) { $('project-error').textContent = error.message; }
}
function bootEditor() {
  if (typeof require !== 'function') {
    $('editor-loading').classList.add('visible');
    return;
  }
  require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs' } });
  require(['vs/editor/editor.main', 'vs/basic-languages/rust/rust'], (_editor, rust) => {
    monaco.languages.register({ id: 'rust' });
    monaco.languages.setLanguageConfiguration('rust', rust.conf);
    monaco.languages.setMonarchTokensProvider('rust', rust.language);
    state.editor = monaco.editor.create($('editor'), {
      value: '', language: 'plaintext', theme: 'vs', automaticLayout: true,
      minimap: { enabled: false }, fontSize: 13, lineNumbers: 'on',
      scrollBeyondLastLine: false, wordWrap: 'off', tabSize: 2,
      renderLineHighlight: 'line', padding: { top: 12 },
    });
    state.editor.onDidChangeModelContent(() => {
      if (state.activePath) setDirty(true);
      const position = state.editor.getPosition();
      if (position) $('cursor-position').textContent = `Ln ${position.lineNumber}, Col ${position.column}`;
    });
    window.addEventListener('resize', () => setDevice(state.device));
    $('editor-loading').classList.remove('visible');
    $('editor-empty').classList.remove('hidden');
    if (state.activePath) state.editor.setModel(monaco.editor.createModel(state.pendingFile, languageFor(state.activePath), monaco.Uri.from({ scheme: 'file', path: `/${state.activePath}` })));
    state.editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, saveFile);
  }, () => {
    $('editor-loading').classList.add('visible');
    $('editor-loading').innerHTML = '<strong>Monaco Editor could not load</strong><span>Check access to the editor CDN and reload the workbench.</span>';
  });
}
document.querySelectorAll('.device-option').forEach(button => button.addEventListener('click', () => setDevice(button.dataset.device)));
document.querySelectorAll('.inspector-tab').forEach(button => button.addEventListener('click', () => {
  state.inspectorTab = button.dataset.inspectorTab;
  document.querySelectorAll('.inspector-tab').forEach(tab => tab.classList.toggle('active', tab === button));
  renderInspector();
}));
$('toggle-inspector').addEventListener('click', () => setInspectorVisible(!state.inspectorVisible));
$('close-inspector').addEventListener('click', () => setInspectorVisible(false));
$('page-select').addEventListener('change', event => { state.page = event.target.value; reloadPreview(); });
$('switch-project').addEventListener('click', () => { $('project-dialog').showModal(); $('project-parent').value = ''; loadProjectChoices(); });
$('browse-projects').addEventListener('click', event => { event.preventDefault(); $('project-error').textContent = ''; loadProjectChoices(); });
$('project-parent').addEventListener('keydown', event => { if (event.key === 'Enter') { event.preventDefault(); loadProjectChoices(); } });
$('load-project').addEventListener('click', event => { event.preventDefault(); switchProject(); });
$('save-file').addEventListener('click', saveFile);
$('reload-preview').addEventListener('click', reloadPreview);
$('refresh-tree').addEventListener('click', loadTree);
$('fullscreen-preview').addEventListener('click', () => window.open($('preview-frame').src, '_blank', 'noopener'));
window.addEventListener('message', event => {
  if (event.origin !== location.origin || event.source !== $('preview-frame').contentWindow || event.data?.source !== 'sdkwork-preview') return;
  if (event.data.type === 'inspector-hide') {
    setInspectorVisible(false);
    return;
  }
  if (event.data.type === 'node-selected') {
    state.selectedNode = event.data.nodeId;
    renderInspector();
  }
});
window.addEventListener('keydown', event => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') { event.preventDefault(); saveFile(); }
});
window.addEventListener('beforeunload', event => {
  if (state.dirty) { event.preventDefault(); event.returnValue = ''; }
});
bootEditor();
loadTree();
loadPages().then(reloadPreview).catch(error => toast(`Could not load pages: ${error.message}`));
setDevice('phone');
";
