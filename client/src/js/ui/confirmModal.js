let resolvePromise = null;

export function initConfirmModal() {
    document.getElementById('btn-confirm-yes').onclick = () => {
        document.getElementById('confirm-modal').classList.add('hidden');
        if (resolvePromise) resolvePromise(true);
    };
    document.getElementById('btn-confirm-no').onclick = () => {
        document.getElementById('confirm-modal').classList.add('hidden');
        if (resolvePromise) resolvePromise(false);
    };
}

export function showConfirm(message, title = 'Confirmar acción') {
    document.getElementById('confirm-modal-title').innerText = title;
    document.getElementById('confirm-modal-message').innerText = message;
    document.getElementById('confirm-modal').classList.remove('hidden');
    return new Promise((resolve) => {
        resolvePromise = resolve;
    });
}