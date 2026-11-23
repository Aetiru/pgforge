import { vi } from "vitest";

// Node 21+ trae `navigator.language` según el sistema; sin esto las pruebas que verifican textos en
// español correrían en inglés en la máquina de quien las ejecuta.
vi.stubGlobal("navigator", { language: "es" });
