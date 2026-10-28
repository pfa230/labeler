# Proposal

## Why

MCP clients can preview a single label or print a sheet, but cannot obtain a filled sheet PDF for download. The REST service already renders that file.

Source issue: https://github.com/pfa230/labeler/issues/506

Fixes #506

## What Changes

- Add a read-only `render_sheet` MCP tool accepting a template, label entries and an optional start slot.
- Return the existing REST sheet PDF as an embedded MCP binary resource without printing.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mcp`: expose sheet PDF download through the existing stdio adapter.

## Impact

The MCP adapter, its mock and real-service tests, and the MCP specification change. Rendering, printing, authentication and the UI keep their existing contracts. No additional dependencies or storage service are needed.
