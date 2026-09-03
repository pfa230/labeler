import "react-data-grid/lib/styles.css";
import { useMemo } from "react";
import { DataGrid, type Column, type RenderEditCellProps, type RenderCellProps, type RowsChangeData } from "react-data-grid";
import type { LabelGridRow } from "../lib/labelGrid";
import type { InputControl, InputSpec } from "../api/types";

const rowKeyGetter = (r: LabelGridRow) => r.id; // stable module-level identity (avoids grid recalculation)

export interface LabelGridProps {
  rows: LabelGridRow[];
  fields: string[];
  optionNames?: string[];
  optionValues?: Record<string, string[]>; // allowed values per declared option
  cellInput?: (row: LabelGridRow, field: string) => InputSpec | undefined;
  // RDG passes the full updated rows plus which indexes changed, so the caller can normalize edited rows.
  onRowsChange: (rows: LabelGridRow[], data: RowsChangeData<LabelGridRow>) => void;
  onDuplicate: (id: string) => void;
  onRemove: (id: string) => void;
  disabled?: boolean; // read-only while a batch is in flight (no editing/duplicate/remove)
  selectedRowId?: string; // which row feeds the label preview
  onSelectRow?: (id: string) => void; // when provided, a leading radio column is rendered
}

const cellErrorStyle = { color: "var(--bad)" } as const;
// Namespaced column keys so a CSV/template field literally named "actions"/"annotation"/"data:x"/"__preview"
// cannot collide with the grid's own columns. Keys are decoded back to field/option names in the cells.
const DATA_PREFIX = "data:";
const OPTION_PREFIX = "option:";

interface DataEditCellProps extends RenderEditCellProps<LabelGridRow> {
  control?: InputControl;
}

function DataEditCell({ row, column, onRowChange, onClose, control = "text" }: DataEditCellProps) {
  const field = column.key.slice(DATA_PREFIX.length);
  const value = row.data[field] !== undefined ? String(row.data[field]) : "";

  if (control === "textarea") {
    return (
      <textarea
        autoFocus
        aria-label={`edit ${field}`}
        className="w-full h-full bg-transparent px-2 resize-none"
        value={value}
        onChange={(e) => onRowChange({ ...row, data: { ...row.data, [field]: e.target.value } })}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            if (e.shiftKey) {
              e.stopPropagation();
            } else {
              e.preventDefault();
            }
          }
        }}
        onBlur={() => onClose(true)}
      />
    );
  }

  return (
    <input
      autoFocus
      aria-label={`edit ${field}`}
      className="w-full bg-transparent px-2"
      value={value}
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
  const value = row.option[name] ?? (typeof row.data[name] === "string" ? String(row.data[name]) : "");
  // Render the current value even if it is not allowed, so an invalid CSV value stays selectable/visible.
  const options = allowed.includes(value) ? allowed : [value, ...allowed];
  return (
    <select
      autoFocus
      aria-label={`edit ${name}`}
      className="w-full bg-transparent px-2"
      value={value}
      onChange={(e) =>
        onRowChange(
          {
            ...row,
            option: { ...row.option, [name]: e.target.value },
            data: { ...row.data, [name]: e.target.value },
          },
          true,
        )
      }
    >
      {options.map((v) => (
        <option key={v} value={v}>
          {v === "" ? "(none)" : v}
        </option>
      ))}
    </select>
  );
}

export function LabelGrid({
  rows,
  fields,
  optionNames = [],
  optionValues = {},
  cellInput,
  onRowsChange,
  onDuplicate,
  onRemove,
  disabled,
  selectedRowId,
  onSelectRow,
}: LabelGridProps) {
  // Memoized so react-data-grid does not recalculate columns on every render (it keys off array identity).
  const columns = useMemo<Column<LabelGridRow>[]>(() => {
    const selectColumn: Column<LabelGridRow> | null = onSelectRow
      ? {
          key: "__preview",
          name: "",
          width: 36,
          renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
            const idx = rows.findIndex((r) => r.id === row.id);
            return (
              <input
                type="radio"
                name="preview-row"
                aria-label={`preview row ${idx + 1}`}
                checked={row.id === selectedRowId}
                onChange={() => onSelectRow(row.id)}
                disabled={disabled}
              />
            );
          },
        }
      : null;

    return [
    ...(selectColumn ? [selectColumn] : []),
    ...fields.map<Column<LabelGridRow>>((field) => ({
      key: `${DATA_PREFIX}${field}`,
      name: field,
      editable: (row: LabelGridRow) => {
        if (disabled) return false;
        if (!cellInput) return true;
        const spec = cellInput(row, field);
        return spec !== undefined && spec.control !== "list";
      },
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        const spec = cellInput ? cellInput(row, field) : { name: field, control: "text" as const };
        if (!spec || spec.control === "list") {
          return <span style={{ color: "var(--muted)", opacity: 0.35 }}>—</span>;
        }
        const err = row.validation.field?.[field];
        const rawValue = row.data[field] ?? "";
        const strValue = String(rawValue);

        // An empty required field renders an explicit, accessible marker (not just a tooltip on empty text).
        if (err && strValue === "") {
          return (
            <span style={cellErrorStyle} aria-label={`${field} ${err}`} title={err}>
              ⚠ {err}
            </span>
          );
        }

        const lines = strValue.split(/\r\n|\n/);
        const isMultiline = lines.length > 1;
        const firstLine = lines[0];
        const remaining = lines.length - 1;

        const title = err && isMultiline
          ? `${err}\n\n${strValue}`
          : (err || (isMultiline ? strValue : undefined));

        if (isMultiline) {
          return (
            <span style={err ? cellErrorStyle : undefined} title={title}>
              <span>{firstLine}</span>{" "}
              <span style={{ color: "var(--muted)", opacity: 0.6 }}>
                +{remaining}
              </span>
            </span>
          );
        }

        return <span style={err ? cellErrorStyle : undefined} title={title}>{strValue}</span>;
      },
      renderEditCell: (p: RenderEditCellProps<LabelGridRow>) => {
        if (disabled) return null;
        const spec = cellInput ? cellInput(p.row, field) : { name: field, control: "text" as const };
        if (!spec || spec.control === "list") return null;
        return <DataEditCell {...p} control={spec.control} />;
      },
    })),
    ...optionNames.map<Column<LabelGridRow>>((name) => ({
      key: `${OPTION_PREFIX}${name}`,
      name: `option.${name}`,
      renderCell: ({ row }: RenderCellProps<LabelGridRow>) => {
        const err = row.validation.option?.[name];
        return <span style={err ? cellErrorStyle : undefined} title={err}>{row.option[name] ?? ""}</span>;
      },
      renderEditCell: disabled || (optionValues[name]?.length ?? 0) <= 1 ? undefined : (p: RenderEditCellProps<LabelGridRow>) => OptionEditCell(p, optionValues[name] ?? []),
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
    ];
  }, [fields, optionNames, optionValues, cellInput, onDuplicate, onRemove, disabled, rows, selectedRowId, onSelectRow]);

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
