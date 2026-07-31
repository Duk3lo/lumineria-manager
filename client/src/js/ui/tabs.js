export function switchTab(tab) {
    const btnLocal = document.getElementById('btn-nav-local');
    const btnRemote = document.getElementById('btn-nav-remote');
    const viewConn = document.getElementById('view-connection');
    const setupLocal = document.getElementById('local-setup');
    const setupRemote = document.getElementById('remote-setup');
    const viewGrid = document.getElementById('view-grid');
    const viewDetail = document.getElementById('view-server-detail');
    const title = document.getElementById('conn-title');

    if (tab === 'local') {
        if (btnLocal) btnLocal.classList.add('active');
        if (btnRemote) btnRemote.classList.remove('active');
        
        if (viewGrid) viewGrid.classList.add('hidden');
        if (viewDetail) viewDetail.classList.add('hidden');
        if (viewConn) viewConn.classList.remove('hidden');
        
        if (setupLocal) setupLocal.classList.remove('hidden');
        if (setupRemote) setupRemote.classList.add('hidden');
        if (title) title.innerText = "Modo Local";
    } else if (tab === 'remote') {
        if (btnRemote) btnRemote.classList.add('active');
        if (btnLocal) btnLocal.classList.remove('active');
        
        if (viewGrid) viewGrid.classList.add('hidden');
        if (viewDetail) viewDetail.classList.add('hidden');
        if (viewConn) viewConn.classList.remove('hidden');
        
        if (setupLocal) setupLocal.classList.add('hidden');
        if (setupRemote) setupRemote.classList.remove('hidden');
        if (title) title.innerText = "Modo Remoto";
    }
}

export function initTabs() {
    const btnLocal = document.getElementById('btn-nav-local');
    const btnRemote = document.getElementById('btn-nav-remote');

    if (btnLocal) btnLocal.onclick = () => switchTab('local');
    if (btnRemote) btnRemote.onclick = () => switchTab('remote');
}