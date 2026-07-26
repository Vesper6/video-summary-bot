const API = '/api';

let vms = [];
let selected = null;
let pollTimer = null;      // VM 状态轮询（starting/running 时 1.5s 一次）
let consoleSource = null;  // SSE 控制台流（就绪后替代轮询刷日志）
let sseFailed = false;     // SSE 连接失败 → 本会话退回轮询

const $ = (sel) => document.querySelector(sel);
const vmList = $('#vm-list');
const vmEmpty = $('#vm-empty');

const stateLabels = {
  created: '已创建',
  starting: '启动中',
  running: '运行中',
  stopping: '停止中',
  stopped: '已停止',
  crashed: '崩溃',
};

async function api(path, opts = {}) {
  const res = await fetch(API + path, {
    headers: { 'Content-Type': 'application/json' },
    ...opts,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || res.statusText);
  }
  if (res.status === 204) return null;
  return res.json();
}

function setStatus(msg) {
  $('#status-msg').textContent = msg;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ─────────────────────────────────────────────
// 顶部页面切换（视频总结 / 虚拟机 / 帮助）
// ─────────────────────────────────────────────

document.querySelectorAll('.menu-item[data-view]').forEach((item) => {
  item.addEventListener('click', () => {
    document.querySelectorAll('.menu-item').forEach((m) => m.classList.remove('active'));
    document.querySelectorAll('.view').forEach((v) => v.classList.remove('active'));
    item.classList.add('active');
    $('#view-' + item.dataset.view).classList.add('active');
  });
});

// ─────────────────────────────────────────────
// 虚拟机管理
// ─────────────────────────────────────────────

function updateToolbar() {
  const has = !!selected;
  const vm = vms.find((v) => v.name === selected);
  const busy = vm && (vm.state === 'running' || vm.state === 'starting' || vm.state === 'stopping');
  const running = vm?.state === 'running';

  $('#btn-edit').disabled = !has || busy;
  $('#btn-delete').disabled = !has || busy;
  $('#btn-start').disabled = !has || busy;
  $('#btn-stop').disabled = !has || !running;
  $('#btn-reboot').disabled = !has || !running;
  $('#btn-shutdown').disabled = !has || !running;
}

function renderVmList() {
  vmList.innerHTML = '';
  vmEmpty.classList.toggle('hidden', vms.length > 0);

  for (const vm of vms) {
    const li = document.createElement('li');
    li.className = 'vm-item' + (vm.name === selected ? ' selected' : '');
    li.dataset.name = vm.name;
    const stateClass = vm.state === 'running' ? 'running' : vm.state === 'stopped' ? 'stopped' : 'created';
    li.innerHTML = `
      <div class="vm-item-name">${escapeHtml(vm.name)}</div>
      <div class="vm-item-meta">${vm.cpus} vCPU · ${vm.memory_mb} MB</div>
      <span class="vm-item-state ${stateClass}">${stateLabels[vm.state] || vm.state}</span>
    `;
    li.addEventListener('click', () => selectVm(vm.name));
    vmList.appendChild(li);
  }
  updateToolbar();
}

async function selectVm(name) {
  if (selected !== name) closeConsoleStream();
  selected = name;
  renderVmList();
  try {
    const vm = await api(`/vms/${encodeURIComponent(name)}`);
    showVmInfo(vm);
    if (!consoleSource) {
      showConsole(vm.console_lines.join('\n') || '[vsb] 无日志');
    }
    schedulePolling(vm);
    openConsoleStream(name);
  } catch (e) {
    setStatus('加载 VM 失败: ' + e.message);
  }
}

function showVmInfo(vm) {
  $('#no-selection').classList.add('hidden');
  $('#info-content').classList.remove('hidden');
  $('#info-name').textContent = vm.name;
  $('#info-state').textContent = stateLabels[vm.state] || vm.state;
  $('#info-cpus').textContent = vm.cpus;
  $('#info-memory').textContent = vm.memory_mb + ' MB';
  $('#info-disk').textContent = vm.disk_gb + ' GB';
  $('#info-cmdline').textContent = vm.cmdline || '(默认)';
}

function showConsole(text) {
  const el = $('#console-output');
  el.textContent = text;
  el.scrollTop = el.scrollHeight;
}

function appendConsole(line) {
  const el = $('#console-output');
  el.textContent += (el.textContent ? '\n' : '') + line;
  el.scrollTop = el.scrollHeight;
}

// ── SSE 实时控制台（失败自动退回轮询） ──────

function openConsoleStream(name) {
  if (sseFailed || consoleSource || !window.EventSource) return;
  const src = new EventSource(`${API}/vms/${encodeURIComponent(name)}/console/stream`);
  let first = true;
  src.onmessage = (ev) => {
    if (first) {
      showConsole('');
      first = false;
    }
    appendConsole(ev.data);
  };
  src.onerror = () => {
    src.close();
    if (consoleSource === src) consoleSource = null;
    if (first) sseFailed = true; // 从未收到数据 → 端点不可用，退回轮询
  };
  consoleSource = src;
}

function closeConsoleStream() {
  if (consoleSource) {
    consoleSource.close();
    consoleSource = null;
  }
}

// ── 状态轮询（starting/running/stopping 时 1.5s） ──

function schedulePolling(vm) {
  const active = vm && ['starting', 'running', 'stopping'].includes(vm.state);
  if (active && !pollTimer) {
    pollTimer = setInterval(pollSelected, 1500);
  } else if (!active && pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

async function pollSelected() {
  if (!selected) return;
  try {
    const vm = await api(`/vms/${encodeURIComponent(selected)}`);
    const cached = vms.find((v) => v.name === vm.name);
    if (cached && cached.state !== vm.state) {
      cached.state = vm.state;
      renderVmList();
    }
    showVmInfo(vm);
    if (!consoleSource) {
      showConsole(vm.console_lines.join('\n') || '[vsb] 无日志');
    }
    schedulePolling(vm);
  } catch {
    /* 下次轮询重试 */
  }
}

async function loadVms() {
  vms = await api('/vms');
  if (selected && !vms.find((v) => v.name === selected)) selected = null;
  if (!selected && vms.length) selected = vms[0].name;
  renderVmList();
  if (selected) await selectVm(selected);
}

async function loadSystem() {
  const info = await api('/system/info');
  $('#version').textContent = 'v' + info.version + ' · ' + info.platform;

  const doctor = await api('/doctor');
  const hv = doctor.checks.find((c) => c.name.includes('Hypervisor'));
  const badge = $('#hv-status');
  if (hv?.ok) {
    badge.textContent = 'Hypervisor 可用';
    badge.className = 'badge ok';
  } else {
    badge.textContent = 'Hypervisor 未就绪';
    badge.className = 'badge warn';
  }

  const list = $('#doctor-checks');
  list.innerHTML = doctor.checks
    .map(
      (c) =>
        `<div class="doctor-row"><span class="icon">${c.ok ? '✅' : c.optional ? '⬜' : '❌'}</span><span>${escapeHtml(c.name)}</span><span class="muted">${escapeHtml(c.detail)}</span></div>`
    )
    .join('');
}

async function vmAction(action) {
  if (!selected) return;
  setStatus(`${action} ${selected}…`);
  try {
    await api(`/vms/${encodeURIComponent(selected)}/${action}`, { method: 'POST' });
    await loadVms();
    setStatus(`${action} 完成`);
  } catch (e) {
    setStatus('操作失败: ' + e.message);
  }
}

// VM 面板内标签页
document.querySelectorAll('.tab').forEach((tab) => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.tab').forEach((t) => t.classList.remove('active'));
    document.querySelectorAll('.tab-panel').forEach((p) => p.classList.remove('active'));
    tab.classList.add('active');
    document.getElementById('panel-' + tab.dataset.tab).classList.add('active');
  });
});

// 工具栏
$('#btn-new').addEventListener('click', () => $('#dlg-create').showModal());
$('#btn-cancel-create').addEventListener('click', () => $('#dlg-create').close());
$('#form-create').addEventListener('submit', async (e) => {
  e.preventDefault();
  const fd = new FormData(e.target);
  const body = {
    name: fd.get('name'),
    cpus: Number(fd.get('cpus')),
    memory_mb: Number(fd.get('memory_mb')),
    disk_gb: Number(fd.get('disk_gb')),
  };
  try {
    await api('/vms', { method: 'POST', body: JSON.stringify(body) });
    $('#dlg-create').close();
    e.target.reset();
    await loadVms();
    selected = body.name;
    await selectVm(selected);
    setStatus(`已创建 VM「${body.name}」`);
  } catch (err) {
    setStatus('创建失败: ' + err.message);
  }
});

$('#btn-edit').addEventListener('click', () => {
  const vm = vms.find((v) => v.name === selected);
  if (!vm) return;
  const form = $('#form-edit');
  $('#edit-vm-name').textContent = '· ' + vm.name;
  form.elements.cpus.value = vm.cpus;
  form.elements.memory_mb.value = vm.memory_mb;
  form.elements.disk_gb.value = vm.disk_gb;
  form.elements.cmdline.value = vm.cmdline || '';
  $('#dlg-edit').showModal();
});
$('#btn-cancel-edit').addEventListener('click', () => $('#dlg-edit').close());
$('#form-edit').addEventListener('submit', async (e) => {
  e.preventDefault();
  if (!selected) return;
  const fd = new FormData(e.target);
  const body = {
    cpus: Number(fd.get('cpus')),
    memory_mb: Number(fd.get('memory_mb')),
    disk_gb: Number(fd.get('disk_gb')),
    cmdline: fd.get('cmdline'),
  };
  try {
    await api(`/vms/${encodeURIComponent(selected)}`, { method: 'PUT', body: JSON.stringify(body) });
    $('#dlg-edit').close();
    await loadVms();
    setStatus(`已更新 VM「${selected}」`);
  } catch (err) {
    setStatus('更新失败: ' + err.message);
  }
});

$('#btn-delete').addEventListener('click', async () => {
  if (!selected || !confirm(`确定删除 VM「${selected}」？`)) return;
  try {
    await api(`/vms/${encodeURIComponent(selected)}`, { method: 'DELETE' });
    closeConsoleStream();
    selected = null;
    $('#no-selection').classList.remove('hidden');
    $('#info-content').classList.add('hidden');
    await loadVms();
    setStatus('已删除');
  } catch (e) {
    setStatus('删除失败: ' + e.message);
  }
});

$('#btn-start').addEventListener('click', () => vmAction('start'));
$('#btn-stop').addEventListener('click', () => vmAction('stop'));
$('#btn-reboot').addEventListener('click', () => vmAction('reboot'));
$('#btn-shutdown').addEventListener('click', () => vmAction('shutdown'));
$('#btn-refresh').addEventListener('click', async () => {
  setStatus('刷新中…');
  await Promise.all([loadVms(), loadSystem()]);
  setStatus('已刷新');
});

// ─────────────────────────────────────────────
// 视频总结页
// ─────────────────────────────────────────────

/** 极简 Markdown 渲染（标题/粗体/行内代码/列表/分隔线/段落）。 */
function renderMarkdown(md) {
  const inline = (s) =>
    escapeHtml(s)
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/`([^`]+)`/g, '<code>$1</code>');

  const lines = md.split(/\r?\n/);
  const out = [];
  let list = null; // 'ul' | 'ol'
  const closeList = () => {
    if (list) {
      out.push(`</${list}>`);
      list = null;
    }
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    const h = line.match(/^(#{1,4})\s+(.*)$/);
    const ul = line.match(/^\s*[-*]\s+(.*)$/);
    const ol = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (h) {
      closeList();
      const n = h[1].length;
      out.push(`<h${n}>${inline(h[2])}</h${n}>`);
    } else if (/^\s*(---|\*\*\*)\s*$/.test(line)) {
      closeList();
      out.push('<hr>');
    } else if (ul) {
      if (list !== 'ul') {
        closeList();
        out.push('<ul>');
        list = 'ul';
      }
      out.push(`<li>${inline(ul[1])}</li>`);
    } else if (ol) {
      if (list !== 'ol') {
        closeList();
        out.push('<ol>');
        list = 'ol';
      }
      out.push(`<li>${inline(ol[1])}</li>`);
    } else if (line.trim() === '') {
      closeList();
    } else {
      closeList();
      out.push(`<p>${inline(line)}</p>`);
    }
  }
  closeList();
  return out.join('\n');
}

$('#form-summarize').addEventListener('submit', async (e) => {
  e.preventDefault();
  const fd = new FormData(e.target);
  const body = {
    url: fd.get('url'),
    level: fd.get('level'),
    language: fd.get('language'),
    no_subtitle: fd.get('no_subtitle') === 'on',
  };

  $('#summary-placeholder').classList.add('hidden');
  $('#summary-result').classList.add('hidden');
  $('#summary-loading').classList.remove('hidden');
  $('#btn-summarize').disabled = true;
  setStatus('正在总结视频…');

  try {
    const res = await api('/summarize', { method: 'POST', body: JSON.stringify(body) });
    const header = [
      res.title ? `<h1>${escapeHtml(res.title)}</h1>` : '',
      `<p class="muted">模型：${escapeHtml(res.model)} · 字幕：${res.subtitle_fetched ? '已获取' : '未获取（基于标题/知识分析）'}</p><hr>`,
    ].join('');
    $('#summary-result').innerHTML = header + renderMarkdown(res.markdown);
    $('#summary-result').classList.remove('hidden');
    setStatus('总结完成');
  } catch (err) {
    $('#summary-result').innerHTML = `<p class="summary-error">❌ 总结失败：${escapeHtml(err.message)}</p>`;
    $('#summary-result').classList.remove('hidden');
    setStatus('总结失败');
  } finally {
    $('#summary-loading').classList.add('hidden');
    $('#btn-summarize').disabled = false;
  }
});

// ─────────────────────────────────────────────
// 初始化
// ─────────────────────────────────────────────

(async () => {
  try {
    await Promise.all([loadVms(), loadSystem()]);
    setStatus('就绪');
  } catch (e) {
    setStatus('无法连接 API: ' + e.message);
  }
})();
