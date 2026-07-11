<script lang="ts">
  import { squarify, type TreemapRect } from "./treemap";
  import { fmtBytes } from "./fmt";
  import type { NodeView } from "./api";

  let { node, onOpen }: { node: NodeView; onOpen: (path: string) => void } = $props();

  const W = 920;
  const H = 420;
  let canvas: HTMLCanvasElement;
  let rects: TreemapRect[] = [];

  $effect(() => {
    if (canvas) draw(node);
  });

  function draw(n: NodeView) {
    const ctx = canvas.getContext("2d")!;
    ctx.clearRect(0, 0, W, H);
    rects = squarify(
      n.entries.map((e) => ({ key: e.path, value: e.size })),
      0, 0, W, H,
    );
    rects.forEach((r, i) => {
      const e = n.entries.find((x) => x.path === r.key)!;
      ctx.fillStyle = e.is_dir
        ? `hsl(${(i * 47) % 360} 55% 52%)`
        : `hsl(${(i * 47) % 360} 15% 62%)`;
      ctx.fillRect(r.x + 1, r.y + 1, Math.max(r.w - 2, 0), Math.max(r.h - 2, 0));
      if (r.w > 70 && r.h > 20) {
        ctx.fillStyle = "#fff";
        ctx.font = "12px system-ui";
        ctx.fillText(`${e.name} ${fmtBytes(e.size)}`, r.x + 5, r.y + 15, r.w - 10);
      }
    });
  }

  function click(ev: MouseEvent) {
    const b = canvas.getBoundingClientRect();
    // CSS 축소 표시 시 내부 좌표계(920x420)로 환산
    const px = (ev.clientX - b.left) * (W / b.width);
    const py = (ev.clientY - b.top) * (H / b.height);
    const hit = rects.find(
      (r) => px >= r.x && px < r.x + r.w && py >= r.y && py < r.y + r.h,
    );
    if (!hit) return;
    const e = node.entries.find((en) => en.path === hit.key);
    if (e?.is_dir) onOpen(e.path);
  }
</script>

<canvas bind:this={canvas} width={W} height={H} onclick={click}></canvas>

<style>
  canvas { max-width: 100%; cursor: pointer; }
</style>
