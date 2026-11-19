import { describe, expect, it } from "vitest";
import { scriptFolderName } from "./script-folder";

// Mismos casos que `pgforge_core::scripts::folder_name` (`crates/pgforge-core/src/scripts.rs`),
// porque esto existe solo para reconocer una carpeta que ya escribió esa función del lado de Rust.
describe("scriptFolderName", () => {
  it("reemplaza los caracteres inválidos de Windows", () => {
    expect(scriptFolderName("prod\\api:db")).toBe("prod_api_db");
    expect(scriptFolderName('a*b?c"d<e>f|g')).toBe("a_b_c_d_e_f_g");
  });

  it("recorta el punto y el espacio del final", () => {
    expect(scriptFolderName("servidor. ")).toBe("servidor");
    expect(scriptFolderName("servidor.. ")).toBe("servidor");
  });

  it("los nombres reservados de Windows no se usan tal cual", () => {
    expect(scriptFolderName("CON")).toBe("CON_");
    expect(scriptFolderName("com3")).toBe("com3_");
    expect(scriptFolderName("Connections")).toBe("Connections");
  });

  it("un nombre vacío tras sanear no devuelve cadena vacía", () => {
    expect(scriptFolderName("...")).toBe("_");
    expect(scriptFolderName("   ")).toBe("_");
  });

  it("un nombre sin nada especial queda igual", () => {
    expect(scriptFolderName("Local")).toBe("Local");
  });
});

import { countEmpty, countVisible, visibleFiles } from "./script-folder";
import type { ScriptEntry, ScriptFolder } from "./ipc";

describe("biblioteca de scripts: vacíos y orden", () => {
  const file = (name: string, modified: number, empty = false): ScriptEntry => ({
    name,
    path: `/x/${name}`,
    modified,
    empty,
    preview: "",
  });
  const folder = (files: ScriptEntry[], folders: ScriptFolder[] = []): ScriptFolder => ({
    name: "c",
    path: "/x",
    files,
    folders,
  });

  it("esconde los vacíos y deja lo último tocado primero", () => {
    const names = visibleFiles([
      file("script1.sql", 10),
      file("script2.sql", 30),
      file("script3.sql", 20, true),
    ]).map((f) => f.name);
    expect(names).toEqual(["script2.sql", "script1.sql"]);
  });

  it("a igual fecha ordena por número y no por texto", () => {
    const names = visibleFiles([file("script10.sql", 5), file("script2.sql", 5)]).map((f) => f.name);
    expect(names).toEqual(["script2.sql", "script10.sql"]);
  });

  it("cuenta visibles y vacíos incluyendo subcarpetas", () => {
    const tree = folder([file("a.sql", 1), file("b.sql", 1, true)], [folder([file("c.sql", 1, true)])]);
    expect(countVisible(tree)).toBe(1);
    expect(countEmpty([tree])).toBe(2);
  });
});
