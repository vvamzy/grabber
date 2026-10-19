'use strict';

// Dashboard logic. Polls the service API and renders the daily-stats cards/table
// and the live downloads table. No framework, no build step.

const $ = (sel) => document.querySelector(sel);

function fmtBytes(n) {
  if (!Number.isFinite(n) || n <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), units.length - 1);
  const v = n / 1024 ** i;
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}
function fmtBps(b) {
  if (!Number.isFinite(b) || b <= 0) return '0 B/s';
  return `${fmtBytes(b)}/s`;
}
function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ---- add download ----
$('#addForm').addEventListener('submit', async (e) => {
  e.preventDefault();
  const url = $('#url').value.trim();
  const segments = Number($('#segments').value) || 8;
  if (!url) return;
  $('#url').value = '';
  await fetch('/api/downloads', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url, segments }),
  });
  refresh();
});

// ---- render downloads ----
async function renderDownloads() {
  let list;
  try { list = await (await fetch('/api/downloads')).json(); } catch { return; }
  const tbody = $('#dlTable tbody');
  tbody.innerHTML = '';
  $('#emptyMsg').classList.toggle('hidden', list.length > 0);

  for (const r of list) {
    const tr = document.createElement('tr');
    const pct = r.total_bytes
      ? Math.min(100, Math.round((r.downloaded_bytes / r.total_bytes) * 100))
      : (r.status === 'completed' ? 100 : 0);

    tr.innerHTML = `
      <td title="${escapeHtml(r.url)}">${escapeHtml(r.filename)}</td>
      <td>${r.total_bytes ? fmtBytes(r.total_bytes) : '&ndash;'}</td>
      <td>
        <div class="bar"><span style="width:${pct}%"></span></div>
        <small>${pct}% &middot; ${fmtBytes(r.downloaded_bytes)}</small>
      </td>
      <td>${r.status === 'running' ? fmtBps(r.avg_bps) : '&ndash;'}</td>
      <td>${fmtBps(r.peak_bps)}</td>
      <td><span class="status ${r.status}">${r.status}</span>${r.error ? ` <small>${escapeHtml(r.error)}</small>` : ''}</td>
      <td>${r.status === 'running'
          ? `<button class="rowbtn" data-cancel="${r.id}">Cancel</button>`
          : `<button class="rowbtn" data-del="${r.id}">Delete</button>`}</td>
    `;
    tbody.appendChild(tr);
  }

  tbody.querySelectorAll('[data-cancel]').forEach((b) =>
    b.addEventListener('click', () => fetch(`/api/downloads/${b.dataset.cancel}/cancel`, { method: 'POST' }).then(refresh)));
  tbody.querySelectorAll('[data-del]').forEach((b) =>
    b.addEventListener('click', () => fetch(`/api/downloads/${b.dataset.del}`, { method: 'DELETE' }).then(refresh)));
}

// ---- render daily stats ----
async function renderStats() {
  let days;
  try { days = await (await fetch('/api/stats/daily')).json(); } catch { return; }
  const tbody = $('#dailyTable tbody');
  tbody.innerHTML = '';

  const todayKey = new Date().toLocaleDateString('en-CA'); // YYYY-MM-DD local
  const today = days.find((d) => d.day === todayKey);

  $('#todayCount').textContent = today ? today.downloads : 0;
  $('#todayBytes').textContent = today ? fmtBytes(today.total_bytes) : '0 B';
  $('#todayTop').textContent = today ? fmtBps(today.top_bps) : '0 B/s';

  for (const d of days) {
    const tr = document.createElement('tr');
    tr.innerHTML = `
      <td>${escapeHtml(d.day)}</td>
      <td>${d.downloads}</td>
      <td>${fmtBytes(d.total_bytes)}</td>
      <td>${fmtBps(d.top_bps)}</td>
      <td>${d.top_speed_name ? escapeHtml(d.top_speed_name) : '&ndash;'}</td>
    `;
    tbody.appendChild(tr);
  }
}

async function pingService() {
  try {
    const r = await (await fetch('/api/ping')).json();
    $('#svcBadge').textContent = r.ok ? 'service online' : 'service error';
    $('#svcBadge').style.color = r.ok ? 'var(--green)' : 'var(--red)';
  } catch {
    $('#svcBadge').textContent = 'service offline';
    $('#svcBadge').style.color = 'var(--red)';
  }
}

function refresh() { renderDownloads(); renderStats(); pingService(); }

refresh();
setInterval(refresh, 1000);
