import { describe, expect, it } from "vitest";
import { fold, rank, score, type Command, type CommandGroup } from "./palette";

function command(label: string, group: CommandGroup = "acción", hint?: string): Command {
  return { id: label, label, group, hint, run: () => {} };
}

const labels = (list: Command[]) => list.map((item) => item.label);

describe("fold", () => {
  it("saca los acentos y las mayúsculas", () => {
    expect(fold("Configuración")).toBe("configuracion");
    expect(fold("ÍNDICES")).toBe("indices");
  });
});

describe("score", () => {
  it("encuentra por subsecuencia, no solo por pedazo entero", () => {
    expect(score("nc", "Nueva consulta")).not.toBeNull();
    expect(score("pedi", "pedidos")).not.toBeNull();
  });

  it("no encuentra lo que no está", () => {
    expect(score("zzz", "Nueva consulta")).toBeNull();
    // El orden importa: las letras tienen que aparecer en la misma secuencia, y en «consulta» la
    // ele viene después de la ce.
    expect(score("lc", "consulta")).toBeNull();
  });

  it("prefiere lo que empieza con lo escrito", () => {
    const empieza = score("mon", "Monitoreo")!;
    const adentro = score("mon", "Panel de monitoreo")!;
    expect(empieza).toBeLessThan(adentro);
  });

  it("prefiere las letras seguidas antes que las desparramadas", () => {
    const seguidas = score("pedi", "pedidos")!;
    const sueltas = score("pedi", "panel de indices")!;
    expect(seguidas).toBeLessThan(sueltas);
  });

  it("con lo escrito vacío no descarta nada", () => {
    expect(score("", "cualquier cosa")).toBe(0);
    expect(score("   ", "cualquier cosa")).toBe(0);
  });

  it("ignora los acentos de los dos lados", () => {
    expect(score("configuracion", "Configuración")).not.toBeNull();
    expect(score("índice", "indice de trabajos")).not.toBeNull();
  });
});

describe("rank", () => {
  it("con la caja vacía respeta el orden en que se ofrecieron", () => {
    const lista = [command("Nueva consulta"), command("Monitoreo", "vista")];
    expect(labels(rank(lista, ""))).toEqual(["Nueva consulta", "Monitoreo"]);
  });

  it("deja afuera lo que no coincide", () => {
    const lista = [command("Nueva consulta"), command("Monitoreo", "vista")];
    expect(labels(rank(lista, "monit"))).toEqual(["Monitoreo"]);
  });

  it("lo que coincide por el nombre va antes que lo que coincide por la pista", () => {
    const lista = [
      command("Datos de clientes", "objeto", "pedidos"),
      command("pedidos", "objeto", "public"),
    ];
    expect(labels(rank(lista, "pedidos"))[0]).toBe("pedidos");
  });

  it("a igual coincidencia manda el tipo de comando", () => {
    const lista = [
      command("ventas", "objeto"),
      command("ventas", "servidor"),
      command("ventas", "acción"),
    ];
    expect(rank(lista, "ventas").map((item) => item.group)).toEqual([
      "acción",
      "servidor",
      "objeto",
    ]);
  });

  it("corta la lista para no dibujar cien filas", () => {
    const lista = Array.from({ length: 100 }, (_, index) => command(`consulta ${index}`));
    expect(rank(lista, "consulta", 5)).toHaveLength(5);
    expect(rank(lista, "", 5)).toHaveLength(5);
  });
});
