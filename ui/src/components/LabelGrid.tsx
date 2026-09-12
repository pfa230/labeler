import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import {
  Grid,
  type IApi,
  type ICellProps,
  type IColumnConfig,
  type IRow,
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

interface GridContextValue extends LabelGridProps {
  commitCell: (rowId: string, field: string, value: string) => void;
}

const GRID_LABEL = "label rows";
const cellErrorStyle = { color: "var(--bad)" } as const;
const inertStyle = { color: "var(--muted)", opacity: 0.35 } as const;
const editableControlClass =
  "w-full rounded border border-input bg-background px-2 py-1 text-sm focus-visible:outline-none focus-visible:ring-1";
const editableControlStyle = {
  background: "var(--surface)",
  borderColor: "var(--border)",
  color: "var(--ink)",
} as const;

// Namespaced column keys so a CSV/template field literally named "actions"/"annotation"/"data:x"/"__preview"
// cannot collide with the grid's own columns. Keys are decoded back to field names in the cells.
const DATA_PREFIX = "data:";
const PREVIEW_COLUMN = "__preview";
const ANNOTATION_COLUMN = "__annotation";
const ACTIONS_COLUMN = "__actions";

const fieldOf = (columnId: unknown) => String(columnId).slice(DATA_PREFIX.length);
// The grid types every row as an open bag; every row this one holds is a LabelGridRow.
const asRow = (row: IRow) => row as unknown as LabelGridRow;

const GridPropsContext = createContext<GridContextValue | null>(null);

function useGridProps(): GridContextValue {
  const props = useContext(GridPropsContext);
  if (!props) throw new Error("a LabelGrid cell rendered outside LabelGrid");
  return props;
}

type CheckboxState = "checked" | "unchecked" | "unset";

function parseCheckboxState(val: unknown): CheckboxState | null {
  if (val === true || val === "true" || val === "1" || val === 1) return "checked";
  if (val === false || val === "false" || val === "0" || val === 0) return "unchecked";
  if (val === "" || val === undefined || val === null) return "unset";
  return null;
}

function nextCheckboxState(current: CheckboxState | null): CheckboxState {
  switch (current) {
    case "unset":
      return "checked";
    case "checked":
      return "unchecked";
    case "unchecked":
      return "unset";
    default:
      return "checked";
  }
}

function checkboxStateToString(state: CheckboxState): string {
  switch (state) {
    case "checked":
      return "true";
    case "unchecked":
      return "false";
    case "unset":
      return "";
  }
}

function computeNumericInvalid(val: string, min?: number, max?: number, isInteger?: boolean): boolean {
  if (val === "") return false;
  const num = Number(val);
  if (Number.isNaN(num)) return true;
  if (isInteger && !Number.isInteger(num)) return true;
  if (min !== undefined && num < min) return true;
  if (max !== undefined && num > max) return true;
  return false;
}

function handleControlKeyDown(e: KeyboardEvent) {
  if (
    e.key === "ArrowUp" ||
    e.key === "ArrowDown" ||
    e.key === "ArrowLeft" ||
    e.key === "ArrowRight" ||
    e.key === "Home" ||
    e.key === "End"
  ) {
    e.stopPropagation();
  }
}

const handleMouseDown = (e: MouseEvent) => e.stopPropagation();
const handleClick = (e: MouseEvent) => e.stopPropagation();
const handleFocus = (e: FocusEvent) => e.stopPropagation();

function DataCell({ row, column }: ICellProps) {
  const { cellInput, disabled, commitCell } = useGridProps();
  const labelRow = asRow(row);
  const field = fieldOf(column.id);
  const cellRef = useRef<HTMLElement | null>(null);
  const setCellRef = useCallback((el: HTMLElement | null) => {
    cellRef.current = el;
  }, []);

  const spec = cellInput ? cellInput(labelRow, field) : { name: field, control: "text" as const };
  const isOperable = Boolean(spec && spec.control !== "list" && spec.control !== "image" && !disabled);

  // Synchronize aria-readonly on the cell wrapper
  useLayoutEffect(() => {
    const wrapper = cellRef.current?.closest('[role="gridcell"]');
    if (wrapper) {
      wrapper.setAttribute("aria-readonly", isOperable ? "false" : "true");
    }
  });

  const err = labelRow.validation.field?.[field];
  const errId = err ? `err-${labelRow.id}-${field}` : undefined;
  const rawVal = labelRow.data[field];
  const strValue = rawVal !== undefined && rawVal !== null ? String(rawVal) : "";

  // Read-only cases: no spec, list, image, or disabled
  if (!spec) {
    return (
      <span ref={setCellRef} style={inertStyle}>
        —
      </span>
    );
  }

  if (spec.control === "list") {
    const text = Array.isArray(rawVal) ? displayCellText(rawVal) : undefined;
    return (
      <span ref={setCellRef} style={text ? undefined : inertStyle}>
        {text ?? "—"}
      </span>
    );
  }

  if (disabled) {
    const lines = strValue.split(/\r\n|\n/);
    const isMultiline = lines.length > 1;
    const firstLine = lines[0];
    const remaining = lines.length - 1;
    const title = err && isMultiline
      ? `${err}\n\n${strValue}`
      : (err || (isMultiline ? strValue : undefined));

    if (isMultiline) {
      return (
        <span ref={setCellRef} style={err ? cellErrorStyle : undefined} title={title}>
          <span>{firstLine}</span>{" "}
          <span style={{ color: "var(--muted)", opacity: 0.6 }}>
            +{remaining}
          </span>
        </span>
      );
    }

    return (
      <span ref={setCellRef} style={err ? cellErrorStyle : undefined} title={title}>
        {strValue}
      </span>
    );
  }

  if (spec.control === "image") {
    return (
      <span ref={setCellRef} style={err ? cellErrorStyle : undefined} title={err || undefined}>
        {strValue !== "" ? "image" : ""}
      </span>
    );
  }

  // Operable controls
  if (spec.control === "checkbox") {
    const checkboxState = parseCheckboxState(rawVal);
    const isUnrepresentable =
      rawVal !== "" && rawVal !== undefined && rawVal !== null && checkboxState === null;
    const state = isUnrepresentable ? "unset" : (checkboxState ?? "unset");
    const adornId = isUnrepresentable ? `adorn-${labelRow.id}-${field}` : undefined;
    const describedBy = [adornId, errId].filter(Boolean).join(" ") || undefined;
    const title = err
      ? (isUnrepresentable ? `${err}\n\n${strValue}` : err)
      : (isUnrepresentable ? strValue : undefined);

    return (
      <div ref={setCellRef} className="flex items-center gap-1.5 w-full" title={title}>
        <input
          ref={(el) => {
            if (el) {
              el.indeterminate = state === "unset";
            }
          }}
          type="checkbox"
          aria-label={`edit ${field}`}
          aria-describedby={describedBy}
          aria-invalid={err ? "true" : undefined}
          checked={state === "checked"}
          onChange={() => {}}
          onClick={(e) => {
            e.stopPropagation();
            const next = nextCheckboxState(isUnrepresentable ? "unset" : checkboxState);
            commitCell(labelRow.id, field, checkboxStateToString(next));
          }}
          onMouseDown={handleMouseDown}
          onFocus={handleFocus}
          onKeyDown={handleControlKeyDown}
          className="rounded border border-input bg-background text-sm cursor-pointer"
        />
        {isUnrepresentable && (
          <span
            id={adornId}
            title={strValue}
            style={{ color: "var(--muted)" }}
            className="text-xs truncate"
          >
            {strValue}
          </span>
        )}
        {err && (
          <span
            id={errId}
            style={cellErrorStyle}
            aria-label={`${field} ${err}`}
            title={err}
            className="text-xs whitespace-nowrap flex-shrink-0"
          >
            ⚠ {err}
          </span>
        )}
      </div>
    );
  }

  if (spec.control === "select") {
    const declaredValues = spec.values ?? [];
    const options: string[] = [""];
    for (const v of declaredValues) {
      if (!options.includes(v)) options.push(v);
    }
    if (strValue !== "" && !options.includes(strValue)) {
      options.push(strValue);
    }
    const title = err ? (strValue !== "" ? `${err}\n\n${strValue}` : err) : undefined;

    return (
      <div ref={setCellRef} className="flex items-center gap-1.5 w-full" title={title}>
        <select
          aria-label={`edit ${field}`}
          aria-describedby={errId}
          aria-invalid={err ? "true" : undefined}
          value={strValue}
          onChange={(e) => commitCell(labelRow.id, field, e.target.value)}
          onMouseDown={handleMouseDown}
          onClick={handleClick}
          onFocus={handleFocus}
          onKeyDown={handleControlKeyDown}
          className={editableControlClass}
          style={editableControlStyle}
        >
          {options.map((v) => (
            <option key={v} value={v}>
              {v === "" ? "(none)" : v}
            </option>
          ))}
        </select>
        {err && (
          <span
            id={errId}
            style={cellErrorStyle}
            aria-label={`${field} ${err}`}
            title={err}
            className="text-xs whitespace-nowrap flex-shrink-0"
          >
            ⚠ {err}
          </span>
        )}
      </div>
    );
  }

  if (spec.control === "integer" || spec.control === "number") {
    const isInteger = spec.control === "integer";
    const isUnrepresentable = strValue !== "" && Number.isNaN(Number(strValue));
    const numInvalid = computeNumericInvalid(strValue, spec.min, spec.max, isInteger);
    const adornId = isUnrepresentable ? `adorn-${labelRow.id}-${field}` : undefined;
    const describedBy = [adornId, errId].filter(Boolean).join(" ") || undefined;
    const title = err
      ? (isUnrepresentable ? `${err}\n\n${strValue}` : err)
      : (isUnrepresentable ? strValue : undefined);

    return (
      <div ref={setCellRef} className="flex items-center gap-1.5 w-full" title={title}>
        <input
          type="number"
          aria-label={`edit ${field}`}
          aria-describedby={describedBy}
          aria-invalid={err || numInvalid ? "true" : undefined}
          min={spec.min}
          max={spec.max}
          step={isInteger ? 1 : "any"}
          value={isUnrepresentable ? "" : strValue}
          onChange={(e) => commitCell(labelRow.id, field, e.target.value)}
          onMouseDown={handleMouseDown}
          onClick={handleClick}
          onFocus={handleFocus}
          onKeyDown={handleControlKeyDown}
          className={editableControlClass}
          style={editableControlStyle}
        />
        {isUnrepresentable && (
          <span
            id={adornId}
            title={strValue}
            style={{ color: "var(--muted)" }}
            className="text-xs truncate"
          >
            {strValue}
          </span>
        )}
        {err && (
          <span
            id={errId}
            style={cellErrorStyle}
            aria-label={`${field} ${err}`}
            title={err}
            className="text-xs whitespace-nowrap flex-shrink-0"
          >
            ⚠ {err}
          </span>
        )}
      </div>
    );
  }

  if (spec.control === "date" || spec.control === "datetime") {
    const isDate = spec.control === "date";
    const isUnrepresentable =
      strValue !== "" &&
      (isDate
        ? !/^\d{4}-\d{2}-\d{2}$/.test(strValue)
        : !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?$/.test(strValue));
    const adornId = isUnrepresentable ? `adorn-${labelRow.id}-${field}` : undefined;
    const describedBy = [adornId, errId].filter(Boolean).join(" ") || undefined;
    const title = err
      ? (isUnrepresentable ? `${err}\n\n${strValue}` : err)
      : (isUnrepresentable ? strValue : undefined);

    return (
      <div ref={setCellRef} className="flex items-center gap-1.5 w-full" title={title}>
        <input
          type={isDate ? "date" : "datetime-local"}
          aria-label={`edit ${field}`}
          aria-describedby={describedBy}
          aria-invalid={err ? "true" : undefined}
          value={isUnrepresentable ? "" : strValue}
          onChange={(e) => commitCell(labelRow.id, field, e.target.value)}
          onMouseDown={handleMouseDown}
          onClick={handleClick}
          onFocus={handleFocus}
          onKeyDown={handleControlKeyDown}
          className={editableControlClass}
          style={editableControlStyle}
        />
        {isUnrepresentable && (
          <span
            id={adornId}
            title={strValue}
            style={{ color: "var(--muted)" }}
            className="text-xs truncate"
          >
            {strValue}
          </span>
        )}
        {err && (
          <span
            id={errId}
            style={cellErrorStyle}
            aria-label={`${field} ${err}`}
            title={err}
            className="text-xs whitespace-nowrap flex-shrink-0"
          >
            ⚠ {err}
          </span>
        )}
      </div>
    );
  }

  if (spec.control === "textarea") {
    const lines = strValue.split(/\r\n|\n/);
    const isMultiline = lines.length > 1;
    const remaining = lines.length - 1;
    const title = err && isMultiline
      ? `${err}\n\n${strValue}`
      : (err || (isMultiline ? strValue : undefined));

    return (
      <div ref={setCellRef} className="flex items-center gap-1.5 w-full h-full" title={title}>
        <textarea
          aria-label={`edit ${field}`}
          aria-describedby={errId}
          aria-invalid={err ? "true" : undefined}
          value={strValue}
          onChange={(e) => commitCell(labelRow.id, field, e.target.value)}
          onMouseDown={handleMouseDown}
          onClick={handleClick}
          onFocus={handleFocus}
          onKeyDown={handleControlKeyDown}
          className={`${editableControlClass} resize-none h-full`}
          style={editableControlStyle}
        />
        {isMultiline && (
          <span
            style={{ color: "var(--muted)", opacity: 0.6 }}
            className="text-xs flex-shrink-0"
          >
            +{remaining}
          </span>
        )}
        {err && (
          <span
            id={errId}
            style={cellErrorStyle}
            aria-label={`${field} ${err}`}
            title={err}
            className="text-xs whitespace-nowrap flex-shrink-0"
          >
            ⚠ {err}
          </span>
        )}
      </div>
    );
  }

  // Default: text
  const lines = strValue.split(/\r\n|\n/);
  const isMultiline = lines.length > 1;
  const firstLine = lines[0];
  const remaining = lines.length - 1;
  const title = err && isMultiline
    ? `${err}\n\n${strValue}`
    : (err || (isMultiline ? strValue : undefined));

  return (
    <div ref={setCellRef} className="flex items-center gap-1.5 w-full" title={title}>
      <input
        type="text"
        aria-label={`edit ${field}`}
        aria-describedby={errId}
        aria-invalid={err ? "true" : undefined}
        value={isMultiline ? firstLine : strValue}
        onChange={(e) => commitCell(labelRow.id, field, e.target.value)}
        onMouseDown={handleMouseDown}
        onClick={handleClick}
        onFocus={handleFocus}
        onKeyDown={handleControlKeyDown}
        className={editableControlClass}
        style={editableControlStyle}
      />
      {isMultiline && (
        <span
          style={{ color: "var(--muted)", opacity: 0.6 }}
          className="text-xs flex-shrink-0"
        >
          +{remaining}
        </span>
      )}
      {err && (
        <span
          id={errId}
          style={cellErrorStyle}
          aria-label={`${field} ${err}`}
          title={err}
          className="text-xs whitespace-nowrap flex-shrink-0"
        >
          ⚠ {err}
        </span>
      )}
    </div>
  );
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
  const { rows, fields, disabled, onSelectRow } = props;

  const latest = useRef(props);
  useEffect(() => {
    latest.current = props;
  });

  const apiRef = useRef<IApi | null>(null);

  const commitCell = useCallback((rowId: string, field: string, value: string) => {
    if (apiRef.current) {
      apiRef.current.exec("update-cell", {
        id: rowId,
        column: `${DATA_PREFIX}${field}`,
        value,
      });
    } else {
      const { rows: current, onRowsChange } = latest.current;
      const index = current.findIndex((r) => r.id === rowId);
      if (index === -1) return;
      onRowsChange(
        current.map((r, i) => (i === index ? { ...r, data: { ...r.data, [field]: value } } : r)),
        { indexes: [index] },
      );
    }
  }, []);

  const columns = useMemo<IColumnConfig[]>(() => [
    ...(onSelectRow ? [{ id: PREVIEW_COLUMN, header: "", width: 36, cell: PreviewCell }] : []),
    ...fields.map((field): IColumnConfig => ({
      id: `${DATA_PREFIX}${field}`,
      header: field,
      flexgrow: 1,
      getter: (row: IRow) => (asRow(row).data[field] ?? "") as Value,
      cell: DataCell,
    })),
    { id: ANNOTATION_COLUMN, header: "Status", flexgrow: 1, cell: AnnotationCell },
    { id: ACTIONS_COLUMN, header: "", width: 110, cell: ActionsCell },
  ], [fields, onSelectRow]);

  const init = useCallback((api: IApi) => {
    apiRef.current = api;

    api.intercept("hotkey", (ev) => {
      const target = ev.event?.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT")
      ) {
        return false;
      }
      return true;
    });

    api.intercept("update-cell", (ev) => {
      const { rows: current, onRowsChange } = latest.current;
      const index = current.findIndex((r) => r.id === ev.id);
      if (index === -1) return false;
      const field = fieldOf(ev.column);
      const value = ev.value as string;
      onRowsChange(
        current.map((r, i) => (i === index ? { ...r, data: { ...r.data, [field]: value } } : r)),
        { indexes: [index] },
      );
      return false;
    });
  }, []);

  const viewport = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    viewport.current?.querySelector('[role="grid"]')?.setAttribute("aria-label", GRID_LABEL);
  }, []);

  const initialFocusDone = useRef(false);
  useEffect(() => {
    if (!initialFocusDone.current && !onSelectRow && !disabled && rows.length > 0) {
      const firstControl = viewport.current?.querySelector<HTMLElement>(
        '[role="gridcell"] input:not([type="radio"]):not([disabled]), [role="gridcell"] textarea:not([disabled]), [role="gridcell"] select:not([disabled])'
      );
      if (firstControl) {
        initialFocusDone.current = true;
        firstControl.focus();
      }
    }
  }, [onSelectRow, disabled, rows]);

  const contextValue = useMemo<GridContextValue>(() => ({
    ...props,
    commitCell,
  }), [props, commitCell]);

  return (
    <GridPropsContext.Provider value={contextValue}>
      <div className="label-grid-viewport" ref={viewport}>
        <Grid data={rows as unknown as IRow[]} columns={columns} select={false} init={init} />
      </div>
    </GridPropsContext.Provider>
  );
}
