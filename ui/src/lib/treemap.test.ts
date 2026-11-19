import { describe, expect, it } from "vitest";
import { squarify } from "./treemap";

const box = { x: 0, y: 0, w: 400, h: 200 };

describe("squarify", () => {
  it("reparte toda el área en rectángulos proporcionales a los valores", () => {
    const values = [60, 30, 10];
    const rects = squarify(values, box);
    const areas = rects.map((r) => r.w * r.h);
    const total = areas.reduce((a, b) => a + b, 0);
    expect(total).toBeCloseTo(400 * 200, 3);
    expect(areas[0] / total).toBeCloseTo(0.6, 3);
    expect(areas[2] / total).toBeCloseTo(0.1, 3);
  });

  it("devuelve los rectángulos en el orden de entrada", () => {
    const rects = squarify([5, 50, 20], box);
    expect(rects[1].w * rects[1].h).toBeGreaterThan(rects[2].w * rects[2].h);
    expect(rects[2].w * rects[2].h).toBeGreaterThan(rects[0].w * rects[0].h);
  });

  it("no se sale de la caja ni se pisan", () => {
    const rects = squarify([40, 25, 15, 10, 6, 4], box);
    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-9);
      expect(r.y).toBeGreaterThanOrEqual(-1e-9);
      expect(r.x + r.w).toBeLessThanOrEqual(400 + 1e-9);
      expect(r.y + r.h).toBeLessThanOrEqual(200 + 1e-9);
    }
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i];
        const b = rects[j];
        const overlapW = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
        const overlapH = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
        expect(overlapW > 1e-6 && overlapH > 1e-6).toBe(false);
      }
    }
  });

  it("los valores en cero o sin caja no reciben área", () => {
    expect(squarify([0, 0], box).every((r) => r.w === 0 && r.h === 0)).toBe(true);
    const rects = squarify([10, 0, 10], box);
    expect(rects[1].w * rects[1].h).toBe(0);
    expect(squarify([1], { x: 0, y: 0, w: 0, h: 10 })[0].w).toBe(0);
  });
});
