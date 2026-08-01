export function withGuard(btn, fn, loadingLabel = null) {
    if (!btn) return;
    const originalText = btn.textContent;
    btn.onclick = async () => {
        if (btn.disabled) return;
        btn.disabled = true;
        if (loadingLabel) btn.textContent = loadingLabel;
        try {
            await fn();
        } finally {
            btn.disabled = false;
            if (loadingLabel) btn.textContent = originalText;
        }
    };
}