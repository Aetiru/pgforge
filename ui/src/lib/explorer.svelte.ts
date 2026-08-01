import {
  connect as ipcConnect,
  disconnect as ipcDisconnect,
  describeError,
  listProfiles,
  treeChildren,
  type ConnectionProfile,
  type ServerCaps,
  type TreeNode,
  type TreeOptions,
} from "./ipc";

/**
 * Una fila del árbol.
 *
 * Los servidores son las raíces del mismo árbol, estén conectados o no. Tener una lista de
 * servidores aparte del árbol obligaba a entender dos cosas para hacer una: elegir el servidor en
 * un lado y navegarlo en el otro.
 */
export interface Row {
  key: string;
  profileId: string;
  /** `null` en las filas de servidor: son la raíz local, no un objeto del catálogo. */
  node: TreeNode | null;
  level: number;
  label: string;
  detail?: string;
  comment?: string;
  hasChildren: boolean;
  expanded: boolean;
  loading: boolean;
  error?: string;
  children: Row[] | null;
  /** Solo significa algo en las filas de servidor. */
  connected: boolean;
}

function serverRow(profile: ConnectionProfile): Row {
  return {
    key: profile.id,
    profileId: profile.id,
    node: null,
    level: 0,
    label: profile.name,
    detail: `${profile.host}:${profile.port}`,
    hasChildren: true,
    expanded: false,
    loading: false,
    children: null,
    connected: false,
  };
}

function childRow(parent: Row, node: TreeNode): Row {
  return {
    key: `${parent.profileId}:${node.id}`,
    profileId: parent.profileId,
    node,
    level: parent.level + 1,
    label: node.label,
    detail: node.detail,
    comment: node.comment,
    hasChildren: node.hasChildren,
    expanded: false,
    loading: false,
    children: null,
    connected: true,
  };
}

class Explorer {
  profiles = $state<ConnectionProfile[]>([]);
  caps = $state<Record<string, ServerCaps>>({});
  roots = $state<Row[]>([]);
  selected = $state<Row | null>(null);
  options = $state<TreeOptions>({ showSystemSchemas: false });
  search = $state("");

  /** Relee los perfiles guardados sin perder lo que ya estaba abierto en el árbol. */
  async refreshProfiles() {
    const profiles = await listProfiles();
    const previous = new Map(this.roots.map((row) => [row.profileId, row]));

    this.profiles = profiles;
    this.roots = profiles.map((profile) => {
      const existing = previous.get(profile.id);
      if (!existing) return serverRow(profile);
      existing.label = profile.name;
      existing.detail = `${profile.host}:${profile.port}`;
      return existing;
    });
  }

  rowFor(profileId: string): Row | null {
    return this.roots.find((row) => row.profileId === profileId) ?? null;
  }

  isConnected(profileId: string) {
    return this.rowFor(profileId)?.connected ?? false;
  }

  async connect(profile: ConnectionProfile, password?: string) {
    const result = await ipcConnect(profile.id, password);
    this.caps[profile.id] = result.caps;

    const row = this.rowFor(profile.id);
    if (row) {
      row.connected = true;
      row.error = undefined;
      row.children = null;
      await this.toggle(row);
    }
  }

  async disconnect(profileId: string) {
    await ipcDisconnect(profileId);
    const row = this.rowFor(profileId);
    if (row) {
      row.connected = false;
      row.expanded = false;
      row.children = null;
    }
    delete this.caps[profileId];
    if (this.selected?.profileId === profileId) {
      this.selected = null;
    }
  }

  /** `true` si la fila no se puede abrir todavía porque su servidor está desconectado. */
  needsConnection(row: Row) {
    return row.node === null && !row.connected;
  }

  async toggle(row: Row) {
    if (!row.hasChildren || this.needsConnection(row)) return;

    if (row.expanded) {
      row.expanded = false;
      return;
    }
    if (row.children === null) {
      await this.loadChildren(row);
    }
    row.expanded = true;
  }

  async loadChildren(row: Row) {
    row.loading = true;
    row.error = undefined;
    try {
      const nodes = await treeChildren(row.profileId, row.node, this.options);
      row.children = nodes.map((node) => childRow(row, node));
    } catch (error) {
      // El nodo queda expandido y vacío con el motivo a la vista, en vez de fallar en silencio.
      row.error = describeError(error);
      row.children = [];
    } finally {
      row.loading = false;
    }
  }

  async reload(row: Row) {
    row.children = null;
    if (row.expanded) {
      await this.loadChildren(row);
    }
  }

  /** Descarta lo cargado de los servidores conectados; se usa al cambiar las opciones. */
  async reloadAll() {
    for (const root of this.roots) {
      if (!root.connected) continue;
      root.children = null;
      root.expanded = false;
      await this.toggle(root);
    }
    this.selected = null;
  }

  select(row: Row) {
    this.selected = row;
  }
}

export const explorer = new Explorer();

/**
 * Aplana el árbol a la lista de filas visibles.
 *
 * Con búsqueda activa se muestran las coincidencias y sus ancestros, sin importar si el nodo
 * estaba expandido. Solo alcanza a lo que ya se trajo del servidor: el árbol se carga por niveles
 * y buscar en todo el catálogo sería otra cosa.
 */
export function visibleRows(rows: Row[], search = ""): Row[] {
  const needle = search.trim().toLowerCase();
  const out: Row[] = [];

  if (needle === "") {
    const walk = (list: Row[]) => {
      for (const row of list) {
        out.push(row);
        if (row.expanded && row.children) walk(row.children);
      }
    };
    walk(rows);
    return out;
  }

  const collect = (list: Row[], into: Row[]) => {
    for (const row of list) {
      const matched: Row[] = [];
      if (row.children) collect(row.children, matched);

      if (matched.length > 0 || row.label.toLowerCase().includes(needle)) {
        into.push(row, ...matched);
      }
    }
  };
  collect(rows, out);
  return out;
}
