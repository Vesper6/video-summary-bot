const API = '/api';

let vms = [];
let selected = null;

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

function updateToolbar() {
  const has = !!selected;
  const vm = vms.find((v) => v.name === selected);
  const running = vm?.state === 'running';

  $('#btn-edit').disabled = !has;
  $('#btn-delete').disabled = !has;
  $('#btn-start').disabled = !has || running;
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

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

async function selectVm(name) {
  selected = name;
  renderVmList();
  try {
    const vm = await api(`/vms/${encodeURIComponent(name)}`);
    showVmInfo(vm);
    showConsole(vm.console_lines.join('\n') || '[vsb] 无日志');
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

// Tabs
document.querySelectorAll('.tab').forEach((tab) => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.tab').forEach((t) => t.classList.remove('active'));
    document.querySelectorAll('.tab-panel').forEach((p) => p.classList.remove('active'));
    tab.classList.add('active');
    document.getElementById('panel-' + tab.dataset.tab).classList.add('active');
  });
});

// Toolbar
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

$('#btn-delete').addEventListener('click', async () => {
  if (!selected || !confirm(`确定删除 VM「${selected}」？`)) return;
  try {
    await api(`/vms/${encodeURIComponent(selected)}`, { method: 'DELETE' });
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

// Init
(async () => {
  try {
    await Promise.all([loadVms(), loadSystem()]);
    setStatus('就绪');
  } catch (e) {
    setStatus('无法连接 API: ' + e.message);
  }
})();