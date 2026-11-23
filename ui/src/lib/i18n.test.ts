import { describe, expect, it } from "vitest";
import { interpolate, pickLocale, translate } from "./i18n";

describe("i18n", () => {
  it("interpola parámetros y deja a la vista el que falta", () => {
    expect(interpolate("Borrar {name} ({n})", { name: "t", n: 2 })).toBe("Borrar t (2)");
    expect(interpolate("Borrar {name}", {})).toBe("Borrar {name}");
  });

  it("sin entrada en el diccionario usa el texto de origen", () => {
    expect(translate({ Guardar: "Save" }, "Guardar")).toBe("Save");
    expect(translate({ Guardar: "Save" }, "Otro {x}", { x: 1 })).toBe("Otro 1");
    expect(translate(null, "Guardar")).toBe("Guardar");
  });

  it("elige idioma: guardado, luego sistema, español de respaldo", () => {
    expect(pickLocale("en", "es-AR")).toBe("en");
    expect(pickLocale(null, "en-US")).toBe("en");
    expect(pickLocale(null, "fr-FR")).toBe("es");
    expect(pickLocale("xx", undefined)).toBe("es");
  });
});
