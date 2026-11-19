import { describe, expect, it } from "vitest";
import { NavHistory } from "./nav-history";

const open = () => true;

describe("NavHistory", () => {
  it("vuelve y avanza por lo que se fue mirando", () => {
    const nav = new NavHistory();
    ["a", "b", "c"].forEach((k) => nav.push(k));
    expect(nav.back(open)).toBe("b");
    expect(nav.back(open)).toBe("a");
    expect(nav.back(open)).toBeNull();
    expect(nav.forward(open)).toBe("b");
    expect(nav.forward(open)).toBe("c");
    expect(nav.forward(open)).toBeNull();
  });

  it("activar una nueva estando atrás corta lo que había adelante", () => {
    const nav = new NavHistory();
    ["a", "b", "c"].forEach((k) => nav.push(k));
    nav.back(open);
    nav.push("d");
    expect(nav.canGoForward).toBe(false);
    expect(nav.back(open)).toBe("b");
  });

  it("repetir la actual no agrega una entrada", () => {
    const nav = new NavHistory();
    nav.push("a");
    nav.push("a");
    nav.push("b");
    expect(nav.back(open)).toBe("a");
    expect(nav.back(open)).toBeNull();
  });

  it("salta las pestañas cerradas", () => {
    const nav = new NavHistory();
    ["a", "b", "c"].forEach((k) => nav.push(k));
    expect(nav.back((k) => k !== "b")).toBe("a");
  });

  it("al cerrar una pestaña sus entradas salen y no queda una repetida pegada", () => {
    const nav = new NavHistory();
    ["a", "b", "a", "c"].forEach((k) => nav.push(k));
    nav.remove("b");
    expect(nav.back(open)).toBe("a");
    expect(nav.back(open)).toBeNull();
  });

  it("tiene tope", () => {
    const nav = new NavHistory();
    for (let i = 0; i < 80; i++) nav.push(`t${i}`);
    let steps = 0;
    while (nav.back(open) !== null) steps++;
    expect(steps).toBe(49);
  });
});
