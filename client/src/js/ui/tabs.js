export function switchTab(tab) {
    document.getElementById('view-local').style.display = tab === 'local' ? 'block' : 'none';
    document.getElementById('view-remote').style.display = tab === 'remote' ? 'block' : 'none';
    document.getElementById('tab-local').classList.toggle('active', tab === 'local');
    document.getElementById('tab-remote').classList.toggle('active', tab === 'remote');
}

export function initTabs() {
    document.getElementById('tab-local').onclick = () => switchTab('local');
    document.getElementById('tab-remote').onclick = () => switchTab('remote');
}