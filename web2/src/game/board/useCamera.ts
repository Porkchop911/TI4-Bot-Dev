import { useCallback, useEffect, useMemo, useRef, type RefObject } from "react";
import type { TileView } from "../../model";
import { boardFrame, frame } from "./hex";

/**
 * Pan and zoom of the board. The camera lives outside React state: a drag updates the SVG view box
 * directly, so the tiles do not render again. The board moves by drag, and zooms with Ctrl + wheel
 * or with two fingers.
 */
export function useCamera(
  svg: RefObject<SVGSVGElement | null>,
  galaxy: RefObject<HTMLDivElement | null>,
  tiles: TileView[],
) {
  // biome-ignore lint/correctness/useExhaustiveDependencies: the tiles are a new array for every view; the frame changes only with their number
  const box = useMemo(() => boardFrame(tiles), [tiles.length]);
  const camera = useRef({ x: 0, y: 0, k: 1 });
  const dragged = useRef(false);

  const scale = useCallback(() => {
    const element = svg.current;
    const viewBox = element?.getAttribute("viewBox");
    if (!element?.clientWidth || !viewBox) {
      return 1;
    }
    const [, , w, h] = viewBox.split(" ").map(Number);
    return Math.max(w / element.clientWidth, h / element.clientHeight);
  }, [svg]);

  const apply = useCallback(() => {
    const { x, y, k } = camera.current;
    const w = box.w / k;
    const h = box.h / k;
    svg.current?.setAttribute(
      "viewBox",
      `${(box.cx + x - w / 2).toFixed(1)} ${(box.cy + y - h / 2).toFixed(1)} ${w.toFixed(1)} ${h.toFixed(1)}`,
    );
    // Small text is hidden when the board is far away. It comes back when the player goes nearer.
    if (svg.current?.clientWidth) {
      galaxy.current?.classList.toggle("far", 1 / scale() < 1.05);
    }
  }, [box, svg, galaxy, scale]);

  const zoom = useCallback(
    (factor: number) => {
      camera.current.k = Math.max(0.6, Math.min(5, camera.current.k * factor));
      apply();
    },
    [apply],
  );

  /** Frames the given systems, or the whole board. */
  const fit = useCallback(
    (ids?: string[]) => {
      // No system to frame: the whole board.
      const picked = ids ? tiles.filter((tile) => ids.includes(tile.id)) : [];
      const area = picked.length ? frame(picked) : box;
      camera.current = {
        x: area.cx - box.cx,
        y: area.cy - box.cy,
        k: Math.min(5, box.w / area.w, box.h / area.h),
      };
      apply();
    },
    [tiles, box, apply],
  );

  /** For a new task: the whole board if it fits at a readable size, else the systems of the task. */
  const fitFor = useCallback(
    (taskSystems: string[]) => {
      const rect = galaxy.current!.getBoundingClientRect();
      fit(Math.min(rect.width / box.w, rect.height / box.h) < 1 ? taskSystems : undefined);
    },
    [fit, box, galaxy],
  );

  useEffect(() => {
    const element = svg.current;
    if (!element) {
      return;
    }
    let drag: { x: number; y: number; cx: number; cy: number; scale: number } | null = null;
    // Two fingers: the point between them stays under them while their distance sets the zoom.
    let pinch: { distance: number; k: number; scale: number; x: number; y: number } | null = null;
    const pointers = new Map<number, { x: number; y: number }>();
    const startDrag = (at: { x: number; y: number }) => {
      drag = { ...at, cx: camera.current.x, cy: camera.current.y, scale: scale() };
    };
    /** The two fingers: their distance, and their middle from the centre of the board in pixels. */
    const fingers = () => {
      const [a, b] = [...pointers.values()];
      const rect = element.getBoundingClientRect();
      return {
        distance: Math.hypot(a.x - b.x, a.y - b.y) || 1,
        x: (a.x + b.x) / 2 - rect.left - rect.width / 2,
        y: (a.y + b.y) / 2 - rect.top - rect.height / 2,
      };
    };
    const down = (event: PointerEvent) => {
      dragged.current = false;
      if (event.button !== 0) {
        return;
      }
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (pointers.size === 2) {
        const now = fingers();
        const unit = scale();
        pinch = {
          distance: now.distance,
          k: camera.current.k,
          scale: unit,
          x: camera.current.x + now.x * unit,
          y: camera.current.y + now.y * unit,
        };
        drag = null;
        // A pinch never selects.
        dragged.current = true;
      } else if (pointers.size === 1) {
        startDrag({ x: event.clientX, y: event.clientY });
      }
    };
    const move = (event: PointerEvent) => {
      const pointer = pointers.get(event.pointerId);
      if (pointer) {
        pointer.x = event.clientX;
        pointer.y = event.clientY;
      }
      if (pinch && pointers.size === 2) {
        const now = fingers();
        const k = Math.max(0.6, Math.min(5, (pinch.k * now.distance) / pinch.distance));
        const unit = (pinch.scale * pinch.k) / k;
        camera.current = { x: pinch.x - now.x * unit, y: pinch.y - now.y * unit, k };
        return apply();
      }
      if (!drag) {
        return;
      }
      const dx = event.clientX - drag.x;
      const dy = event.clientY - drag.y;
      if (!dragged.current && Math.hypot(dx, dy) < 5) {
        return;
      }
      dragged.current = true;
      camera.current.x = drag.cx - dx * drag.scale;
      camera.current.y = drag.cy - dy * drag.scale;
      apply();
      element.classList.add("dragging");
    };
    const up = (event: PointerEvent) => {
      pointers.delete(event.pointerId);
      pinch = null;
      drag = null;
      // The finger that stays moves the board on, from where it is.
      const [rest] = pointers.values();
      if (rest) {
        startDrag(rest);
      }
      element.classList.remove("dragging");
    };
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey) {
        return;
      }
      event.preventDefault();
      zoom(event.deltaY < 0 ? 1.15 : 1 / 1.15);
    };
    element.addEventListener("pointerdown", down);
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
    document.addEventListener("pointercancel", up);
    element.addEventListener("wheel", wheel, { passive: false });
    window.addEventListener("resize", apply);
    return () => {
      element.removeEventListener("pointerdown", down);
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      document.removeEventListener("pointercancel", up);
      element.removeEventListener("wheel", wheel);
      window.removeEventListener("resize", apply);
    };
  }, [svg, apply, scale, zoom]);

  /** A drag never selects: call this in the capture phase of a click. */
  const wasDrag = useCallback(() => {
    const result = dragged.current;
    dragged.current = false;
    return result;
  }, []);

  return { apply, zoom, fit, fitFor, wasDrag };
}
