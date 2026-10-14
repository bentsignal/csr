# Cosmetic articulated death bodies

The repeated low-violence clip is replaced, when a valid installed rig exists,
by cosmetic rigid bodies. This is a first physics implementation, not proof of
VPhysics parity. No gameplay collision or damage is driven by these bodies.

Runtime-only player MDL/PHY data supplies named solids, convex geometry, masses,
damping, joint limits and collision pairs. Checksums and named bone bindings
must agree. Actual last rendered animation is retained only for the matching
victim life/team; authority supplies death origin, yaw, velocity and hit ray.
Nonphysical fingers/accessories retain captured local transforms under their
physical ancestor. Rewound hit positions have a bounded presentation lever arm.

The solver is the public Apache-2.0 Rust [Rapier](https://rapier.rs/docs/), pinned
to 0.36.0. This does not import Source engine code. Inch/metre conversion is
explicit. Generic joint construction follows the public
[joint documentation](https://rapier.rs/docs/user_guides/rust/joints/).
The installed axis limits are relative to skeletal rest frames; exact VPhysics
constraint-frame/force/inertia correspondence remains uncalibrated. Hit impulse,
friction and restitution are provisional presentation policy.

Each corpse has a bounded 384-inch static neighborhood, 128-Hz stepping with
bounded catch-up, sleep and a 12-second simulation cap. Sixteen bodies expire
after twenty simulation seconds and clear on round/session changes. Geometry
cooking runs on Bevy's asynchronous compute pool so a death cannot block the
render thread. Dropping a body cancels its pending task. Missing/unsupported
rigs retain the prior animation fallback.

A brute-force plane-triple prototype was rejected after a real Dust II prop
with 1,395 bevel planes caused a 23-second creation stall. Incremental clipped
polyhedra plus batched static colliders cut installed creation to 15.4ms median,
30.3ms maximum across sixteen spawn locations; this work is asynchronous.
CPU-only installed checks verify all initial T/CT bone matrices, pause, falling,
settling and bone lengths. The sixteen-body update/pose sample had p95 3.93ms.
These checks do not establish exact original death trajectories. Rendered
inspection and the full workspace/data checks are still required before merge.

Known limits: bodies do not collide with each other or moving world objects;
static neighborhoods do not follow long airborne trajectories; exact 2015
weapon impulses, inertia interpretation and death-pose blending remain open.
