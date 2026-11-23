import type { Dictionary } from "../../i18n";
import { common } from "./common";
import { modules } from "./modules";
import { roles } from "./roles";
import { rolesTree } from "./roles-tree";
import { folderLabels } from "./folders";
import { ui1 } from "./ui-1";
import { ui2 } from "./ui-2";
import { ui3 } from "./ui-3";
import { ui4 } from "./ui-4";
import { ui5 } from "./ui-5";
import { ui6 } from "./ui-6";
import { ui7 } from "./ui-7";
import { ui8 } from "./ui-8";

/**
 * Un archivo por área y no uno solo: es lo que permite traducir en paralelo sin pisarse, y cada
 * uno se lee entero. `ui-N` son lotes de componentes `.svelte`, `modules` lo que arman los `.ts`.
 * La clave es el texto en español exacto del `t("…")`.
 */
export const en: Dictionary = {
  ...common,
  ...modules,
  ...roles,
  ...rolesTree,
  ...folderLabels,
  ...ui1,
  ...ui2,
  ...ui3,
  ...ui4,
  ...ui5,
  ...ui6,
  ...ui7,
  ...ui8,
};
