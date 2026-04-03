// Long-press → contextmenu for touch devices (mobile only)
(function () {
  let timer = null;
  let startX = 0;
  let startY = 0;

  document.addEventListener("touchstart", function (e) {
    if (e.touches.length !== 1) return;
    startX = e.touches[0].clientX;
    startY = e.touches[0].clientY;

    timer = setTimeout(function () {
      timer = null;
      const touch = e.touches[0];
      const target = document.elementFromPoint(touch.clientX, touch.clientY) || e.target;
      const evt = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: touch.clientX,
        clientY: touch.clientY,
        screenX: touch.screenX,
        screenY: touch.screenY,
      });
      target.dispatchEvent(evt);
    }, 500);
  }, { passive: true });

  document.addEventListener("touchmove", function (e) {
    if (!timer) return;
    const dx = e.touches[0].clientX - startX;
    const dy = e.touches[0].clientY - startY;
    if (dx * dx + dy * dy > 100) {
      clearTimeout(timer);
      timer = null;
    }
  }, { passive: true });

  document.addEventListener("touchend", function () {
    if (timer) { clearTimeout(timer); timer = null; }
  });

  document.addEventListener("touchcancel", function () {
    if (timer) { clearTimeout(timer); timer = null; }
  });

  // Suppress default iOS long-press behavior (magnifier, text selection)
  document.addEventListener("contextmenu", function (e) {
    e.preventDefault();
  });
})();
