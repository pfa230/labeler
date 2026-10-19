# Spec Delta

## ADDED Requirements

### Requirement: Print-form list dragging

The print-form list editor defined by "Parameter controls" SHALL offer a drag handle to move an element directly to another element's position. Earlier/later buttons SHALL remain visible and keyboard-operable.

#### Scenario: Dragging moves an empty element past duplicates

- **WHEN** the list holds `["A","B","A",""]` and the operator drags the fourth element to the second position
- **THEN** the displayed and submitted array is `["A","","B","A"]`
