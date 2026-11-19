import type { SchemaRelation } from "../ipc";

/** Un catálogo mínimo para las pruebas del editor. */
export const RELATIONS: SchemaRelation[] = [
  {
    oid: 16401,
    schema: "public",
    name: "clientes",
    comment: "Quien compra",
    columns: [
      { name: "id", typeName: "bigint" },
      { name: "nombre", typeName: "text" },
    ],
  },
  {
    oid: 16410,
    schema: "public",
    name: "pedidos",
    columns: [
      { name: "id", typeName: "bigint" },
      { name: "cliente_id", typeName: "bigint" },
      { name: "total", typeName: "numeric" },
    ],
  },
];
