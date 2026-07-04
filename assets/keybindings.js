// Stop unmodified library shortcut keys (Enter, Backspace, Delete, ArrowUp,
// ArrowDown) from bubbling to the global onkeydown handler when focus is in an
// editable element. Dioxus' SerializedKeyboardData carries no DOM target, so
// the Rust handler can't tell whether the user is typing into a textbox vs.
// pressing a library shortcut — we gate it here in capture phase instead.
//
// Cmd/Ctrl-modified keys are left alone so shortcuts like Cmd+F still work
// while typing.
(function () {
  if (window.__rotero_keybindings) return;
  window.__rotero_keybindings = true;

  const SWALLOW = new Set([
    "Enter",
    "Backspace",
    "Delete",
    "ArrowUp",
    "ArrowDown",
  ]);

  function isEditable(el) {
    if (!el) return false;
    const tag = el.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
    if (el.isContentEditable) return true;
    return false;
  }

  document.addEventListener(
    "keydown",
    function (e) {
      if (e.metaKey || e.ctrlKey) return;
      if (!SWALLOW.has(e.key)) return;
      if (!isEditable(e.target)) return;
      e.stopPropagation();
    },
    true,
  );
})();
