## ADDED Requirements

### Requirement: A trimmed repeat can end with a marker

A repeating container SHALL accept an optional literal string `trim_marker`; a container without `repeat` carrying that key SHALL be refused at load naming the key and its layout path. When the first child discarded by `overflow: trim` is an instance of a repeat carrying `trim_marker`, that repeat SHALL draw its longest prefix of input instances that fits followed by one instance with the repeated name bound to the marker string, using the parent's packing rules. Earlier siblings SHALL retain their places, and children following that repeat SHALL remain trimmed. If no marker instance fits after the earlier siblings, the render SHALL fail with `UnsupportedLayoutItem` reason `item_out_of_frame` at the repeating container's path.

#### Scenario: A marker distinguishes overflow from an exactly fitting list

- **WHEN** a trimming row fits three equal-sized tag instances, a repeating container declares `trim_marker: "..."`, and the marker instance has the same size as a tag
- **THEN** input `[A, B, C, D]` draws `A`, `B`, `...` in the strip
- **AND** input `[A, B, C]` draws `A`, `B`, `C`

#### Scenario: A marker belongs to a repeat

- **WHEN** a container without `repeat` declares `trim_marker: "..."`
- **THEN** the template is refused at load naming that container's `trim_marker` path
