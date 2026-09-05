import { describe, expect, it } from "vitest";
import { childPath, renamedScriptPath, siblingPath } from "./script-name";

describe("renamedScriptPath", () => {
  it("cambia el nombre y conserva el directorio", () => {
    expect(renamedScriptPath("C:\\scripts\\Local\\script1.sql", "reportes")).toBe(
      "C:\\scripts\\Local\\reportes.sql",
    );
  });

  it("agrega .sql si el título no lo trae", () => {
    expect(renamedScriptPath("/scripts/local/script1.sql", "consulta diaria")).toBe(
      "/scripts/local/consulta diaria.sql",
    );
  });

  it("no duplica la extensión si ya la trae", () => {
    expect(renamedScriptPath("/scripts/local/script1.sql", "consulta.sql")).toBe(
      "/scripts/local/consulta.sql",
    );
    expect(renamedScriptPath("/scripts/local/script1.sql", "consulta.SQL")).toBe(
      "/scripts/local/consulta.SQL",
    );
  });

  it("respeta el separador de la ruta original", () => {
    expect(renamedScriptPath("scripts\\local\\a.sql", "b")).toBe("scripts\\local\\b.sql");
    expect(renamedScriptPath("scripts/local/a.sql", "b")).toBe("scripts/local/b.sql");
  });

  it("un título con separadores no crea una subcarpeta", () => {
    expect(renamedScriptPath("/scripts/local/a.sql", "../otra/cosa")).toBe(
      "/scripts/local/.._otra_cosa.sql",
    );
  });

  it("recorta los espacios del título", () => {
    expect(renamedScriptPath("/scripts/local/a.sql", "  con espacios  ")).toBe(
      "/scripts/local/con espacios.sql",
    );
  });
});

describe("siblingPath", () => {
  it("reemplaza el último tramo, sin agregar extensión", () => {
    expect(siblingPath("/scripts/local/sub", "otra")).toBe("/scripts/local/otra");
    expect(siblingPath("C:\\scripts\\Local\\sub", "otra")).toBe("C:\\scripts\\Local\\otra");
  });
});

describe("childPath", () => {
  it("agrega un tramo nuevo al final", () => {
    expect(childPath("/scripts/local", "reportes")).toBe("/scripts/local/reportes");
    expect(childPath("C:\\scripts\\Local", "reportes")).toBe("C:\\scripts\\Local\\reportes");
  });

  it("no duplica el separador si la ruta ya termina en uno", () => {
    expect(childPath("/scripts/local/", "reportes")).toBe("/scripts/local/reportes");
  });
});
