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
