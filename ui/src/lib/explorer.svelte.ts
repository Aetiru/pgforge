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
 * Una fila del árbol. Las filas de servidor tienen `node` en `null`: son la raíz local, no un
 * objeto del catálogo.
 */
export interface Row {
  key: string;
  profileId: string;
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
  };
}

class Explorer {
  profiles = $state<ConnectionProfile[]>([]);
  caps = $state<Record<string, ServerCaps>>({});
  roots = $state<Row[]>([]);
  selected = $state<Row | null>(null);
  options = $state<TreeOptions>({ showSystemSchemas: false });

  async refreshProfiles() {
    this.profiles = await listProfiles();
  }

  isConnected(profileId: string) {
    return this.roots.some((row) => row.profileId === profileId);
  }

  async connect(profile: ConnectionProfile, password?: string) {
    const result = await ipcConnect(profile.id, password);
    this.caps[profile.id] = result.caps;

    const row = serverRow(result.profile);
    // Reconectar reemplaza la fila anterior en su lugar, para que el servidor no salte de posición.
    const existing = this.roots.findIndex((r) => r.profileId === profile.id);
    if (existing >= 0) {
      this.roots[existing] = row;
    } else {
      this.roots.push(row);
    }
    await this.toggle(row);
  }

  async disconnect(profileId: string) {
    await ipcDisconnect(profileId);
    this.roots = this.roots.filter((row) => row.profileId !== profileId);
    delete this.caps[profileId];
    if (this.selected?.profileId === profileId) {
      this.selected = null;
    }
  }

  async toggle(row: Row) {
    if (!row.hasChildren) return;
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

  /** Vuelve a pedir los hijos de un nodo ya cargado. */
  async reload(row: Row) {
    row.children = null;
    if (row.expanded) {
      await this.loadChildren(row);
    }
  }

  /** Descarta todo lo cargado; se usa al cambiar las opciones de visualización. */
  async reloadAll() {
    for (const root of this.roots) {
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

/** Aplana el árbol a la lista de filas visibles, que es lo que se dibuja. */
export function visibleRows(rows: Row[], out: Row[] = []): Row[] {
  for (const row of rows) {
    out.push(row);
    if (row.expanded && row.children) {
      visibleRows(row.children, out);
    }
  }
  return out;
}
