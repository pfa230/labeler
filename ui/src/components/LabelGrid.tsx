import "react-data-grid/lib/styles.css";
import { useMemo } from "react";
import { DataGrid, type Column, type RenderEditCellProps, type RenderCellProps, type RowsChangeData } from "react-data-grid";
import type { LabelGridRow } from "../lib/labelGrid";

const rowKeyGetter = (r: LabelGridRow) => r.id; // stable module-level identity (avoids grid recalculation)

export interface LabelGridProps {
  rows: LabelGridRow[];
  fields: string[];
  optionNames: string[];
  optionValues: Record<string, string[]>; // allowed values per declared option
  // RDG passes the full updated rows plus which indexes changed, so the caller can normalize edited rows.
  onRowsChange: (rows: LabelGridRow[], data: RowsChangeData<LabelGridRow>) => void;
  onDuplicate: (id: string) => void;
  onRemove: (id: string) => void;
  disabled?: boolean; // read-only while a batch is in flight (no editing/duplicate/remove)
}

const cellErrorStyle = { color: "var(--bad)" } as const;
// Namespaced column keys so a CSV/template field literally named "actions"/"annotation"/"data:x"
// cannot collide with the grid's own columns. Keys are decoded back to field/option names in the cells.
const DATA_PREFIX = "data:";
const OPTION_PREFIX = "option:";

function DataEditCell({ row, column, onRowChange, onClose }: RenderEditCellProps<LabelGridRow>) {
  const field = column.key.slice(DATA_PREFIX.length);
  return (
    <input
      autoFocus
      aria-label={`edit ${field}`}
      className="w-full bg-transparent px-2"
      value={row.data[field] ?? ""}
      onChange={(e) => onRowChange({ ...row, data: { ...row.data, [field]: e.target.value } })}
      onBlur={() => onClose(true)}
    />
  );
}

function OptionEditCell(
  { row, column, onRowChange }: RenderEditCellProps<LabelGridRow>,
  allowed: string[],
) {
  const name = column.key.slice(OPTION_PREFIX.length);
  const value = row.option[name] ?? "";
  // Render the current value even if it is not allowed, so an invalid CSV value stays selectable/visible.
  const options = allowed.includes(value) ? allowed : [value, ...allowed];
  return (
    <select
      autoFocus
      aria-label={`edit ${name}`}
      className="w-full bg-transparent px-2"
      value={value}
      onChange={(e) => onRowChange({ ...row, option: { ...row.option, [name]: e.target.value } }, true)}
    >
      {options.map((v) => (
        <option key={v} value={v}>
          {v === "" ? "(none)" : v}
        </option>
      ))}
    </select>
  );
}

export function LabelGrid({ rows, fields, optionNames, optionValues, onRowsChange, onDuplicate, onRemove, disabled }: LabelGridProps) {
  // Memoized so react-data-grid does not recalculate columns on every render (it keys off array identity).
  const columns = useMemo<Column<LabelGridRow>[]>(() => [
    ...fields.map<Column<LabelGridRow>>((field) => ({
      key: `${DATA_PREFIX}${field}`,
      name: field,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        const err = row.validation.field?.[field];
        const value = row.data[field] ?? "";
        // An empty required field renders an explicit, accessible marker (not just a tooltip on empty text).
        if (err && value === "") {
          return (
            <span style={cellErrorStyle} aria-label={`${field} ${err}`} title={err}>
              ⚠ {err}
            </span>
          );
        }
        return <span style={err ? cellErrorStyle : undefined} title={err}>{value}</span>;
      },
      renderEditCell: disabled ? undefined : DataEditCell,
    })),
    ...optionNames.map<Column<LabelGridRow>>((name) => ({
      key: `${OPTION_PREFIX}${name}`,
      name: `option.${name}`,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        const err = row.validation.option?.[name];
        return <span style={err ? cellErrorStyle : undefined} title={err}>{row.option[name] ?? ""}</span>;
      },
      renderEditCell: disabled ? undefined : (p: RenderEditCellProps<LabelGridRow>) => OptionEditCell(p, optionValues[name] ?? []),
    })),
    {
      key: "__annotation",
      name: "Status",
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        if (!row.annotation) return null;
        const ok = row.annotation.status === "ok";
        return (
          <span style={{ color: ok ? "var(--ok, green)" : "var(--bad)" }}>
            {ok ? "ok" : `failed: ${row.annotation.message ?? ""}`}
          </span>
        );
      },
    },
    {
      key: "__actions",
      name: "",
      width: 110,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => (
        <span className="flex gap-2">
          <button type="button" aria-label="duplicate row" disabled={disabled} onClick={() => onDuplicate(row.id)}>
            ⧉
          </button>
          <button type="button" aria-label="remove row" disabled={disabled} onClick={() => onRemove(row.id)}>
            ✕
          </button>
        </span>
      ),
    },
  ], [fields, optionNames, optionValues, onDuplicate, onRemove, disabled]);

  return (
    <DataGrid
      aria-label="label rows"
      columns={columns}
      rows={rows}
      rowKeyGetter={rowKeyGetter}
      onRowsChange={onRowsChange}
      enableVirtualization={false}
    />
  );
}
