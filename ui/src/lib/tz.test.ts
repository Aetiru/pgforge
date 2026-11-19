import { describe, expect, it } from "vitest";
import { convertTimestamptz } from "./tz";

const gmt5 = () => -300; // un equipo en UTC-05, sin depender de la zona de quien corre la prueba

describe("convertTimestamptz", () => {
  it("el servidor deja el texto igual", () => {
    expect(convertTimestamptz("2016-12-22 08:40:00-05", "server")).toBe("2016-12-22 08:40:00-05");
  });

  it("pasa a UTC y cruza el día", () => {
    expect(convertTimestamptz("2016-12-22 22:30:00-05", "utc")).toBe("2016-12-23 03:30:00+00");
  });

  it("conserva los microsegundos", () => {
    expect(convertTimestamptz("2024-01-01 00:00:00.123456+00", "utc")).toBe("2024-01-01 00:00:00.123456+00");
  });

  it("pasa a la zona local con su desfase", () => {
    expect(convertTimestamptz("2016-12-22 13:40:00+00", "local", gmt5)).toBe("2016-12-22 08:40:00-05");
  });

  it("entiende desfases con minutos", () => {
    expect(convertTimestamptz("2024-06-01 12:00:00+05:30", "utc")).toBe("2024-06-01 06:30:00+00");
    expect(convertTimestamptz("2024-06-01 12:00:00+00", "local", () => 330)).toBe("2024-06-01 17:30:00+05:30");
  });

  it("no toca lo que no es un timestamptz", () => {
    for (const value of ["2016-12-22 08:40:00", "2016-12-22", "hola", "", "12:00:00+00"]) {
      expect(convertTimestamptz(value, "utc")).toBe(value);
    }
  });
});
