// Rotero — Graph Canvas Renderer
// Renders force-directed paper graph on HTML canvas with live physics.
// Communicates with Dioxus via dioxus.send().

(function() {
  "use strict";

  let canvas, ctx, dpr;
  let nodes = [], links = [], nodeMap = {};
  let transform = { x: 0, y: 0, scale: 1 };
  let dragNode = null, dragOffset = { x: 0, y: 0 };
  let isPanning = false, panStart = { x: 0, y: 0 };
  let hoverNode = null;
  let highlightIds = null;
  let tooltipEl = null;
  let lastClickTime = 0;
  let animFrameId = null;

  // Physics constants
  const REPULSION = 80;
  const ATTRACTION = 0.0002;
  const CENTER_GRAVITY = 0.001;
  const DAMPING = 0.45;         // very high friction
  const MAX_VELOCITY = 0.08;

  // Ambient wander via simplex noise — very slow, barely perceptible drift
  const WANDER_STRENGTH = 0.03;
  const WANDER_FREQ = 0.00012;
  let simTime = 0;

  // 2D simplex noise (compact implementation)
  const _sn_F2 = 0.5 * (Math.sqrt(3) - 1), _sn_G2 = (3 - Math.sqrt(3)) / 6;
  const _sn_grad = [[1,1],[-1,1],[1,-1],[-1,-1],[1,0],[-1,0],[0,1],[0,-1]];
  const _sn_perm = new Uint8Array(512);
  (function() {
    const p = new Uint8Array(256);
    for (let i = 0; i < 256; i++) p[i] = i;
    for (let i = 255; i > 0; i--) { const j = (i * 48271 + 1) & 255; const t = p[i]; p[i] = p[j]; p[j] = t; }
    for (let i = 0; i < 512; i++) _sn_perm[i] = p[i & 255];
  })();
  function simplex2(xin, yin) {
    const s = (xin + yin) * _sn_F2;
    const i = Math.floor(xin + s), j = Math.floor(yin + s);
    const t = (i + j) * _sn_G2;
    const x0 = xin - (i - t), y0 = yin - (j - t);
    const i1 = x0 > y0 ? 1 : 0, j1 = x0 > y0 ? 0 : 1;
    const x1 = x0 - i1 + _sn_G2, y1 = y0 - j1 + _sn_G2;
    const x2 = x0 - 1 + 2 * _sn_G2, y2 = y0 - 1 + 2 * _sn_G2;
    const ii = i & 255, jj = j & 255;
    let n0 = 0, n1 = 0, n2 = 0;
    let t0 = 0.5 - x0 * x0 - y0 * y0;
    if (t0 > 0) { t0 *= t0; const g = _sn_grad[_sn_perm[ii + _sn_perm[jj]] & 7]; n0 = t0 * t0 * (g[0] * x0 + g[1] * y0); }
    let t1 = 0.5 - x1 * x1 - y1 * y1;
    if (t1 > 0) { t1 *= t1; const g = _sn_grad[_sn_perm[ii + i1 + _sn_perm[jj + j1]] & 7]; n1 = t1 * t1 * (g[0] * x1 + g[1] * y1); }
    let t2 = 0.5 - x2 * x2 - y2 * y2;
    if (t2 > 0) { t2 *= t2; const g = _sn_grad[_sn_perm[ii + 1 + _sn_perm[jj + 1]] & 7]; n2 = t2 * t2 * (g[0] * x2 + g[1] * y2); }
    return 70 * (n0 + n1 + n2); // returns -1..1
  }

  const EDGE_COLORS = {
    tag: "#0d9488",
    collection: "#6366f1",
    author: "#f59e0b",
    journal: "#94a3b8"
  };

  window.__roteroGraph = {
    init: function(canvasId, tooltipId) {
      canvas = document.getElementById(canvasId);
      tooltipEl = document.getElementById(tooltipId);
      if (!canvas) return;
      ctx = canvas.getContext("2d");
      dpr = window.devicePixelRatio || 1;
      resizeCanvas();
      canvas.addEventListener("wheel", onWheel, { passive: false });
      canvas.addEventListener("mousedown", onMouseDown);
      canvas.addEventListener("mousemove", onMouseMove);
      canvas.addEventListener("mouseup", onMouseUp);
      canvas.addEventListener("mouseleave", onMouseLeave);
      window.addEventListener("resize", resizeCanvas);
    },

    setData: function(json) {
      const data = typeof json === "string" ? JSON.parse(json) : json;
      nodes = data.nodes || [];
      links = data.links || [];
      nodeMap = {};
      nodes.forEach((n, idx) => {
        nodeMap[n.id] = n;
        n.vx = 0; n.vy = 0;
        n.noiseSeedX = idx * 73.7;  // unique noise offset per node
        n.noiseSeedY = idx * 73.7 + 500;
      });
      simTime = 0;
      // Center the view
      if (nodes.length > 0) {
        let cx = 0, cy = 0;
        nodes.forEach(n => { cx += n.x; cy += n.y; });
        cx /= nodes.length; cy /= nodes.length;
        transform.x = canvas.width / (2 * dpr) - cx;
        transform.y = canvas.height / (2 * dpr) - cy;
        transform.scale = 1;
      }
      startAnimation();
    },

    highlight: function(ids) {
      highlightIds = ids ? new Set(ids) : null;
    },

    recenter: function() {
      if (nodes.length === 0) return;
      let cx = 0, cy = 0;
      nodes.forEach(n => { cx += n.x; cy += n.y; });
      cx /= nodes.length; cy /= nodes.length;
      transform.x = canvas.width / (2 * dpr) - cx;
      transform.y = canvas.height / (2 * dpr) - cy;
      transform.scale = 1;
    },

    stop: function() {
      if (animFrameId) { cancelAnimationFrame(animFrameId); animFrameId = null; }
    }
  };

  function startAnimation() {
    if (animFrameId) cancelAnimationFrame(animFrameId);
    function loop() {
      simulate();
      draw();
      animFrameId = requestAnimationFrame(loop);
    }
    animFrameId = requestAnimationFrame(loop);
  }

  function simulate() {
    const n = nodes.length;
    if (n === 0 || dragNode || isPanning) return;
    simTime++;

    // Repulsion between all node pairs
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const a = nodes[i], b = nodes[j];
        let dx = a.x - b.x, dy = a.y - b.y;
        let dist = Math.sqrt(dx * dx + dy * dy) || 1;
        let force = REPULSION / (dist * dist);
        let fx = (dx / dist) * force;
        let fy = (dy / dist) * force;
        a.vx += fx; a.vy += fy;
        b.vx -= fx; b.vy -= fy;
      }
    }

    // Attraction along edges
    for (const link of links) {
      const a = nodeMap[link.source], b = nodeMap[link.target];
      if (!a || !b) continue;
      let dx = b.x - a.x, dy = b.y - a.y;
      let dist = Math.sqrt(dx * dx + dy * dy) || 1;
      let force = dist * ATTRACTION * (link.weight || 1);
      let fx = (dx / dist) * force;
      let fy = (dy / dist) * force;
      a.vx += fx; a.vy += fy;
      b.vx -= fx; b.vy -= fy;
    }

    // Gentle center gravity
    let cx = 0, cy = 0;
    nodes.forEach(n => { cx += n.x; cy += n.y; });
    cx /= n; cy /= n;

    const t = simTime * WANDER_FREQ;

    for (const node of nodes) {
      if (node === dragNode) { node.vx = 0; node.vy = 0; continue; }

      // Center gravity
      node.vx -= (node.x - cx) * CENTER_GRAVITY;
      node.vy -= (node.y - cy) * CENTER_GRAVITY;

      // Ambient wander — simplex noise gives smooth, organic paths per node
      node.vx += simplex2(node.noiseSeedX, t) * WANDER_STRENGTH;
      node.vy += simplex2(node.noiseSeedY, t) * WANDER_STRENGTH;

      // Damping + velocity cap
      node.vx *= DAMPING;
      node.vy *= DAMPING;
      let speed = Math.sqrt(node.vx * node.vx + node.vy * node.vy);
      if (speed > MAX_VELOCITY) {
        node.vx = (node.vx / speed) * MAX_VELOCITY;
        node.vy = (node.vy / speed) * MAX_VELOCITY;
      }
      node.x += node.vx;
      node.y += node.vy;
    }
  }

  function resizeCanvas() {
    if (!canvas) return;
    const rect = canvas.parentElement.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    canvas.style.width = rect.width + "px";
    canvas.style.height = rect.height + "px";
  }

  function screenToWorld(sx, sy) {
    return {
      x: (sx - transform.x) / transform.scale,
      y: (sy - transform.y) / transform.scale
    };
  }

  function hitTest(sx, sy) {
    const w = screenToWorld(sx, sy);
    // Use a generous hit area — at least 12px in screen space
    const minR = 12 / transform.scale;
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const dx = w.x - n.x, dy = w.y - n.y;
      const r = Math.max(n.size + 4, minR);
      if (dx * dx + dy * dy <= r * r) return n;
    }
    return null;
  }

  // Precompute hover neighbor set each frame (avoids O(links) per node)
  let hoverNeighborIds = new Set();
  let hoverEdge = null; // the edge currently under the cursor

  function buildHoverNeighbors() {
    hoverNeighborIds.clear();
    if (!hoverNode) return;
    for (const l of links) {
      if (l.source === hoverNode.id) hoverNeighborIds.add(l.target);
      if (l.target === hoverNode.id) hoverNeighborIds.add(l.source);
    }
  }

  function edgeHitTest(sx, sy) {
    const w = screenToWorld(sx, sy);
    const threshold = 4 / transform.scale; // px tolerance
    for (let i = links.length - 1; i >= 0; i--) {
      const l = links[i];
      const a = nodeMap[l.source], b = nodeMap[l.target];
      if (!a || !b) continue;
      const dist = pointToSegmentDist(w.x, w.y, a.x, a.y, b.x, b.y);
      if (dist < threshold) return l;
    }
    return null;
  }

  function pointToSegmentDist(px, py, ax, ay, bx, by) {
    const dx = bx - ax, dy = by - ay;
    const lenSq = dx * dx + dy * dy;
    if (lenSq === 0) return Math.sqrt((px - ax) * (px - ax) + (py - ay) * (py - ay));
    let t = ((px - ax) * dx + (py - ay) * dy) / lenSq;
    t = Math.max(0, Math.min(1, t));
    const cx = ax + t * dx, cy = ay + t * dy;
    return Math.sqrt((px - cx) * (px - cx) + (py - cy) * (py - cy));
  }

  function draw() {
    if (!ctx) return;
    const w = canvas.width, h = canvas.height;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    // Background — solid dark
    const isDark = canvas.closest('.dark') !== null;
    ctx.fillStyle = isDark ? "#1e1e1e" : "#f5f5f5";
    ctx.fillRect(0, 0, w / dpr, h / dpr);

    ctx.save();
    ctx.translate(transform.x, transform.y);
    ctx.scale(transform.scale, transform.scale);

    buildHoverNeighbors();

    // Are we in hover-focus mode or search-highlight mode?
    const hovering = hoverNode !== null;
    const searching = highlightIds !== null;

    // Edge color — muted, Obsidian-style
    const edgeBase = isDark ? "#484848" : "#c0c0c0";
    const edgeHighlight = isDark ? "#777777" : "#999999";

    // Draw edges — thin, subtle
    for (const link of links) {
      const src = nodeMap[link.source];
      const tgt = nodeMap[link.target];
      if (!src || !tgt) continue;

      const isEdgeHovered = link === hoverEdge;
      const connectedToHover = hovering &&
        (link.source === hoverNode.id || link.target === hoverNode.id);
      const searchMatch = searching &&
        highlightIds.has(link.source) && highlightIds.has(link.target);

      let alpha, bright;
      if (isEdgeHovered) {
        alpha = 0.8; bright = true;
      } else if (hoverEdge && !hovering) {
        alpha = 0.06; bright = false;
      } else if (hovering) {
        alpha = connectedToHover ? 0.6 : 0.06; bright = connectedToHover;
      } else if (searching) {
        alpha = searchMatch ? 0.4 : 0.06; bright = searchMatch;
      } else {
        alpha = 0.65; bright = false;
      }

      ctx.beginPath();
      ctx.moveTo(src.x, src.y);
      ctx.lineTo(tgt.x, tgt.y);
      ctx.strokeStyle = alphaColor(bright ? edgeHighlight : edgeBase, alpha);
      ctx.lineWidth = (bright ? 1.8 : 1.2) / transform.scale;
      ctx.stroke();
    }

    // Draw nodes — flat filled circles, no border, no decoration
    const fontSize = Math.max(9, 11 / transform.scale);
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    const labelColor = isDark ? "#dcddde" : "#2e2e2e";

    // Nodes connected to a hovered edge
    const edgeEndpoints = hoverEdge
      ? new Set([hoverEdge.source, hoverEdge.target])
      : null;

    for (const n of nodes) {
      const isHovered = n === hoverNode;
      const isNeighbor = hoverNeighborIds.has(n.id);
      const isEdgeEnd = edgeEndpoints && edgeEndpoints.has(n.id);
      const isSearchMatch = searching && highlightIds.has(n.id);

      // Determine opacity
      let nodeAlpha;
      if (hovering) {
        nodeAlpha = (isHovered || isNeighbor) ? 1.0 : 0.1;
      } else if (hoverEdge) {
        nodeAlpha = isEdgeEnd ? 1.0 : 0.15;
      } else if (searching) {
        nodeAlpha = isSearchMatch ? 1.0 : 0.1;
      } else {
        nodeAlpha = 1.0;
      }

      const r = n.size;

      // Flat filled circle — no stroke, no shadow
      ctx.globalAlpha = nodeAlpha;
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
      ctx.fillStyle = (isHovered || isEdgeEnd) ? lightenColor(n.color, 0.3) : n.color;
      ctx.fill();

      // Label — only on hover or when zoomed in
      if (isHovered || isNeighbor || isEdgeEnd || (transform.scale > 0.8 && nodeAlpha > 0.5)) {
        ctx.font = `${fontSize}px -apple-system, BlinkMacSystemFont, sans-serif`;
        ctx.fillStyle = labelColor;
        ctx.globalAlpha = (isHovered || isEdgeEnd) ? 1.0 : isNeighbor ? 0.9 : nodeAlpha * 0.6;
        ctx.fillText(n.label, n.x, n.y + r + 4);
      }

      ctx.globalAlpha = 1.0;
    }

    ctx.restore();
  }

  function lightenColor(hex, amount) {
    if (!hex || !hex.startsWith("#")) return hex || "#aaa";
    let r = parseInt(hex.slice(1, 3), 16);
    let g = parseInt(hex.slice(3, 5), 16);
    let b = parseInt(hex.slice(5, 7), 16);
    r = Math.min(255, r + (255 - r) * amount);
    g = Math.min(255, g + (255 - g) * amount);
    b = Math.min(255, b + (255 - b) * amount);
    return `rgb(${r|0},${g|0},${b|0})`;
  }

  function alphaColor(hex, alpha) {
    if (!hex || hex.startsWith("rgba")) return hex || "rgba(0,0,0,0)";
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return `rgba(${r},${g},${b},${alpha})`;
  }

  function onWheel(e) {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const zoom = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    transform.x = mx - (mx - transform.x) * zoom;
    transform.y = my - (my - transform.y) * zoom;
    transform.scale *= zoom;
    transform.scale = Math.max(0.1, Math.min(10, transform.scale));
  }

  function onMouseDown(e) {
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const node = hitTest(mx, my);
    if (node) {
      dragNode = node;
      const w = screenToWorld(mx, my);
      dragOffset.x = node.x - w.x;
      dragOffset.y = node.y - w.y;
      canvas.style.cursor = "grabbing";
    } else {
      isPanning = true;
      hoverEdge = null;
      panStart.x = mx - transform.x;
      panStart.y = my - transform.y;
      canvas.style.cursor = "grabbing";
    }
  }

  function onMouseMove(e) {
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    if (dragNode) {
      const w = screenToWorld(mx, my);
      dragNode.x = w.x + dragOffset.x;
      dragNode.y = w.y + dragOffset.y;
      // Freeze all other nodes while dragging
      for (const nd of nodes) { nd.vx = 0; nd.vy = 0; }
      return;
    }

    if (isPanning) {
      transform.x = mx - panStart.x;
      transform.y = my - panStart.y;
      return;
    }

    const node = hitTest(mx, my);
    const edge = node ? null : edgeHitTest(mx, my);

    hoverNode = node;
    hoverEdge = edge;
    canvas.style.cursor = (node || edge) ? "pointer" : "default";

    if (tooltipEl) {
      if (node) {
        const fullNode = nodeMap[node.id];
        tooltipEl.style.display = "block";
        tooltipEl.style.left = (mx + 12) + "px";
        tooltipEl.style.top = (my + 12) + "px";
        tooltipEl.innerHTML =
          '<div class="tooltip-title">' + escapeHtml(fullNode._fullTitle || fullNode.label) + '</div>' +
          '<div class="tooltip-meta">' + escapeHtml(fullNode._authors || '') + '</div>' +
          (fullNode._year ? '<div class="tooltip-meta">' + fullNode._year + '</div>' : '');
      } else {
        tooltipEl.style.display = "none";
      }
    }
  }

  function onMouseUp(e) {
    if (dragNode) {
      try {
        dioxus.send(JSON.stringify({ type: "drag_end", id: dragNode.id, x: dragNode.x, y: dragNode.y }));
      } catch(_) {}
      dragNode = null;
      canvas.style.cursor = "default";
      return;
    }

    if (!isPanning) {
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const node = hitTest(mx, my);
      if (node) {
        const now = Date.now();
        if (now - lastClickTime < 350) {
          try { dioxus.send(JSON.stringify({ type: "dblclick", id: node.id })); } catch(_) {}
        } else {
          try { dioxus.send(JSON.stringify({ type: "click", id: node.id })); } catch(_) {}
        }
        lastClickTime = now;
      }
    }

    isPanning = false;
    canvas.style.cursor = "default";
  }

  function onMouseLeave() {
    isPanning = false;
    dragNode = null;
    hoverNode = null;
    hoverEdge = null;
    if (tooltipEl) tooltipEl.style.display = "none";
    canvas.style.cursor = "default";
  }

  function escapeHtml(s) {
    const div = document.createElement("div");
    div.textContent = s;
    return div.innerHTML;
  }
})();
