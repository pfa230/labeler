## Context

See proposal.md, Why. The Homebox connector builds one link per row from the connection's external origin and the entity id, for browse rows (`url`, plus the `item_url` or `location_url` cell) and for materialized `item_url`/`location_url` fields. Every path is `/entity/<id>` today. The connector already knows each row's resource: it queries `entities` with `isLocation=false` and `locations` with `isLocation=true`.

## Goals / Non-Goals

**Goals:** links open the Homebox page for the row, per the amended "Generated links use the public URL" requirement.

**Non-Goals:** asset-id links (`/a/<assetId>`). Homebox serves them, but the issue asks only that the existing links resolve.

## Decisions

**The resource picks the route.** `entities` maps to `/item/`, `locations` to `/location/`. The alternative, reading each entity's `entityType.isLocation` from the upstream payload, needs a field browse and materialize do not otherwise fetch, and gives the same answer, because the resource's upstream query already filtered on that flag.

**One link builder takes the resource.** Browse and materialize keep calling a single function, now given the resource as well as origin and id, so the two paths cannot disagree on a route.

## Risks / Trade-offs

[Labels printed before this change keep `/entity/` links] → none; they never resolved, and reprinting fixes them.
