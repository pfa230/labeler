import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, type ChangeEvent, type KeyboardEvent } from "react";
import {
  Grid,
  registerInlineEditor,
  type IApi,
  type ICellProps,
  type IColumnConfig,
  type IRow,
  type TEditorConfig,
  type Value,
} from "@svar-ui/react-grid";
import "@svar-ui/react-grid/style.css";
import type { LabelGridRow } from "../lib/labelGrid";
import { displayCellText } from "../lib/connectorRows";
import type { InputSpec } from "../api/types";

// Which rows an edit touched, by index. Import and Connect clear a prior run's annotation on
// exactly those rows, so the shape outlives the grid library that first supplied it.
export interface LabelGridRowsChange {
  indexes: number[];
}

export interface LabelGridProps {
  rows: LabelGridRow[];
  fields: string[];
  cellInput?: (row: LabelGridRow, field: string) => InputSpec | undefined;
  onRowsChange: (rows: LabelGridRow[], change: LabelGridRowsChange) => void;
  onDuplicate: (id: string) => void;
  onRemove: (id: string) => void;
  disabled?: boolean; // read-only while a batch is in flight (no editing/duplicate/remove)
  selectedRowId?: string; // which row feeds the label preview
  onSelectRow?: (id: string) => void; // when provided, a leading radio column is rendered
}

const GRID_LABEL = "label rows";
const cellErrorStyle = { color: "var(--bad)" } as const;
const inertStyle = { color: "var(--muted)", opacity: 0.35 } as const;
// Namespaced column keys so a CSV/template field literally named "actions"/"annotation"/"data:x"/"__preview"
// cannot collide with the grid's own columns. Keys are decoded back to field names in the cells.
const DATA_PREFIX = "data:";
const PREVIEW_COLUMN = "__preview";
const ANNOTATION_COLUMN = "__annotation";
const ACTIONS_COLUMN = "__actions";

const fieldOf = (columnId: unknown) => String(columnId).slice(DATA_PREFIX.length);
// The grid types every row as an open bag; every row this one holds is a LabelGridRow.
const asRow = (row: IRow) => row as unknown as LabelGridRow;

// Cells and editors are module-level components so each column keeps one component identity for the
// grid's whole lifetime: a fresh identity on every render would remount every cell. They read the
// grid's props from here instead of from a per-render closure.
const GridPropsContext = createContext<LabelGridProps | null>(null);

function useGridProps(): LabelGridProps {
  const props = useContext(GridPropsContext);
  if (!props) throw new Error("a LabelGrid cell rendered outside LabelGrid");
  return props;
}

const TEXT_EDITOR = "labeler-text";
const TEXTAREA_EDITOR = "labeler-textarea";

// The runtime hands an inline editor `onSave`/`onApply`/`onCancel`
// (@svar-ui/react-grid/dist/index.es.js:1127-1135) while the shipped types name the same three
// props in lowercase. Follow the runtime, and cast at the single point of contact below.
interface InlineEditorProps {
  editor: TEditorConfig;
  onSave: (ignoreFocus?: boolean) => void;
  onApply: (value: string) => void;
  onCancel: () => void;
}
type VendorInlineEditor = Parameters<typeof registerInlineEditor>[1];

// One component serves both registered editors: the registered name is how a column picks the
// control, and the component reads that choice back off `editor.type`.
//
// The cell around an open editor reads a bubbled Enter as a cancel, and a bubbled Escape reaches the
// grid's document-level hotkeys, so both keys are answered here and go no further.
function CellEditor({ editor, onSave, onApply, onCancel }: InlineEditorProps) {
  const shared = {
    autoFocus: true,
    "aria-label": `edit ${fieldOf(editor.column)}`,
    value: String(editor.value ?? ""),
    onChange: (e: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => onApply(e.target.value),
    onBlur: () => onSave(true),
  };
  const cancel = (e: KeyboardEvent) => {
    e.stopPropagation();
    onCancel();
  };

  if (editor.type === TEXTAREA_EDITOR) {
    return (
      <textarea
        {...shared}
        className="w-full h-full bg-transparent px-2 resize-none"
        onKeyDown={(e) => {
          if (e.key === "Escape") return cancel(e);
          if (e.key !== "Enter") return;
          // Shift+Enter types a newline and commits nothing; plain Enter commits and types nothing.
          e.stopPropagation();
          if (!e.shiftKey) {
            e.preventDefault();
            onSave();
          }
        }}
      />
    );
  }

  return (
    <input
      {...shared}
      className="w-full bg-transparent px-2"
      onKeyDown={(e) => {
        if (e.key === "Escape") return cancel(e);
        if (e.key !== "Enter") return;
        e.stopPropagation();
        onSave();
      }}
    />
  );
}

registerInlineEditor(TEXT_EDITOR, CellEditor as unknown as VendorInlineEditor);
registerInlineEditor(TEXTAREA_EDITOR, CellEditor as unknown as VendorInlineEditor);

function DataCell({ row, column }: ICellProps) {
  const { cellInput } = useGridProps();
  const labelRow = asRow(row);
  const field = fieldOf(column.id);

  const spec = cellInput ? cellInput(labelRow, field) : { name: field, control: "text" as const };
  if (!spec) {
    return <span style={inertStyle}>—</span>;
  }
  if (spec.control === "list") {
    const rawValue = labelRow.data[field];
    if (Array.isArray(rawValue)) {
      return <span>{displayCellText(rawValue)}</span>;
    }
    return <span style={inertStyle}>—</span>;
  }

  const err = labelRow.validation.field?.[field];
  const strValue = String(labelRow.data[field] ?? "");

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
}

function PreviewCell({ row }: ICellProps) {
  const { rows, selectedRowId, onSelectRow, disabled } = useGridProps();
  const id = asRow(row).id;
  const index = rows.findIndex((r) => r.id === id);
  return (
    <input
      type="radio"
      name="preview-row"
      aria-label={`preview row ${index + 1}`}
      checked={id === selectedRowId}
      onChange={() => onSelectRow?.(id)}
      disabled={disabled}
    />
  );
}

function AnnotationCell({ row }: ICellProps) {
  const { annotation } = asRow(row);
  if (!annotation) return null;
  const ok = annotation.status === "ok";
  return (
    <span style={{ color: ok ? "var(--ok, green)" : "var(--bad)" }}>
      {ok ? "ok" : `failed: ${annotation.message ?? ""}`}
    </span>
  );
}

function ActionsCell({ row }: ICellProps) {
  const { onDuplicate, onRemove, disabled } = useGridProps();
  const id = asRow(row).id;
  return (
    <span className="flex gap-2">
      <button type="button" aria-label="duplicate row" disabled={disabled} onClick={() => onDuplicate(id)}>
        ⧉
      </button>
      <button type="button" aria-label="remove row" disabled={disabled} onClick={() => onRemove(id)}>
        ✕
      </button>
    </span>
  );
}

export function LabelGrid(props: LabelGridProps) {
  const { rows, fields, cellInput, disabled, onSelectRow } = props;

  // The intercept below is registered once, on the grid's first render, so it reads the props of the
  // render it fires in rather than the ones it was created with. The ref is filled after the commit
  // rather than during render; nothing can reach the intercept before then, since it only ever runs
  // from an edit the user made.
  const latest = useRef(props);
  useEffect(() => {
    latest.current = props;
  });

  const columns = useMemo<IColumnConfig[]>(() => [
    ...(onSelectRow ? [{ id: PREVIEW_COLUMN, header: "", width: 36, cell: PreviewCell }] : []),
    ...fields.map((field): IColumnConfig => ({
      id: `${DATA_PREFIX}${field}`,
      header: field,
      flexgrow: 1,
      // The grid types a cell value as string | number | Date; a label field is also allowed to hold
      // a boolean or a list, and this value only travels on to this file's own cells and editors.
      //
      // Left unguarded against a missing `data` deliberately. The grid can hand a getter the `{}`
      // that `{...getRow(id)}` yields for a row that is gone, and this line would throw on it, but
      // nothing here reaches that: removing a row unmounts its editor in the same commit, so the
      // `editor` action never recomputes against it; `open-editor` would reach this getter with an
      // undefined row, except that it evaluates this column's `editor` predicate first and the
      // `!row` return there stops it (the same guard covers `getNextEditor`, through
      // `isCellEditable`); and the undo history manager, which resolves rows against a previous
      // state, is never built while the grid mounts with `undo` off. Turn `undo` on, or keep an
      // editor mounted over a row absent from `data`, and a check belongs here (#377).
      getter: (row: IRow) => (asRow(row).data[field] ?? "") as Value,
      // Editability is per row and per column: the grid calls this to decide whether a cell can be
      // edited at all (DataStore.isCellEditable, which is also what makes Tab skip the cells that
      // cannot) and, when it can, which editor opens. It is called with no row by the column helper
      // that feeds @svar-ui/react-editor, so an absent row answers "not editable" rather than throwing.
      editor: (row?: IRow) => {
        if (!row || disabled) return null;
        if (!cellInput) return TEXT_EDITOR;
        const spec = cellInput(asRow(row), field);
        if (!spec || spec.control === "list") return null;
        return spec.control === "textarea" ? TEXTAREA_EDITOR : TEXT_EDITOR;
      },
      cell: DataCell,
    })),
    { id: ANNOTATION_COLUMN, header: "Status", flexgrow: 1, cell: AnnotationCell },
    { id: ACTIONS_COLUMN, header: "", width: 110, cell: ActionsCell },
  ], [fields, cellInput, disabled, onSelectRow]);

  const init = useCallback((api: IApi) => {
    // The grid renders the rows it is handed and never owns them: an edit becomes the caller's
    // onRowsChange and the grid's own row update is cancelled (returning false stops the default
    // handler), so `rows` stays the one source of truth.
    api.intercept("update-cell", (ev) => {
      const { rows: current, onRowsChange } = latest.current;
      const index = current.findIndex((r) => r.id === ev.id);
      // The edited row left the grid between the editor opening and closing; there is nothing to
      // commit it to, so the edit is dropped rather than applied to whichever row now sits there.
      if (index === -1) return false;
      const field = fieldOf(ev.column);
      const value = ev.value as string; // this file's editors only ever apply an input's string value
      onRowsChange(
        current.map((r, i) => (i === index ? { ...r, data: { ...r.data, [field]: value } } : r)),
        { indexes: [index] },
      );
      return false;
    });
  }, []);

  const viewport = useRef<HTMLDivElement>(null);
  // The grid renders its own role="grid" element and forwards no DOM attributes to it (every extra
  // prop is routed to an event handler instead), so its accessible name is applied to that node
  // here. React leaves alone an attribute it never rendered, so this survives the grid's re-renders.
  // Connect.test.tsx finds the grid by this name a dozen times over.
  useLayoutEffect(() => {
    viewport.current?.querySelector('[role="grid"]')?.setAttribute("aria-label", GRID_LABEL);
  }, []);

  return (
    <GridPropsContext.Provider value={props}>
      <div className="label-grid-viewport" ref={viewport}>
        <Grid data={rows as unknown as IRow[]} columns={columns} select={false} init={init} />
      </div>
    </GridPropsContext.Provider>
  );
}
