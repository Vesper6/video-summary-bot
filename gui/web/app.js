const API = '/api';

const $ = (sel) => document.querySelector(sel);

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
// 顶部页面切换（视频总结 / 帮助）
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
// 系统健康信息（doctor）
// ─────────────────────────────────────────────

async function loadSystem() {
  const info = await api('/system/info');
  $('#version').textContent = 'v' + info.version + ' · ' + info.platform;

  const doctor = await api('/doctor');
  const list = $('#doctor-checks');
  list.innerHTML = doctor.checks
    .map(
      (c) =>
        `<div class="doctor-row"><span class="icon">${c.ok ? '✅' : c.optional ? '⬜' : '❌'}</span><span>${escapeHtml(c.name)}</span><span class="muted">${escapeHtml(c.detail)}</span></div>`
    )
    .join('');
}

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
    vision: fd.get('vision') === 'on',
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
      `<p class="muted">模型：${escapeHtml(res.model)} · 字幕：${res.subtitle_fetched ? '已获取' : '未获取（基于标题/知识分析）'}${res.frames_analyzed > 0 ? ` · 画面：${res.frames_analyzed} 帧` : ''}</p><hr>`,
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
    await loadSystem();
    setStatus('就绪');
  } catch (e) {
    setStatus('无法连接 API: ' + e.message);
  }
})();
