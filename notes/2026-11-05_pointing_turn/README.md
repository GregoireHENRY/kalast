# 2026-11-05 — `kalast.shape.pointing`: the body's turn in each image, fitted with its pointing

A body whose rotation model is uncertain is seen turned in each image from where its model puts it. A roll about the boresight (2026-11-04) mends only the part about the line of
sight; the rest moves the face seen as the body turns about its own origin. SPC solves this with each image's view
of the body; `pointing` now fits it with the image's shift.

What changed (`src/shape/pointing.rs`):

- `s[k, t] = P[k] + D[k, t] d[k] + J[k, t] o[t]`: `d[k]` a small rotation vector (radians, the surface's frame)
  turning the body about its origin; the maplet at `c[t]` moves by `d[k] x c[t]`, `D[k, t]` the projection of that
  move (three columns, one an axis).
- Freed image by image (`turn`, a flag a camera), as the roll: shift and turn by weighted least squares (5 x 5, the
  Huber loss's weights, the wrong locks past three Huber lengths left out), a weak hold on the turn (0.3 rad for a
  pixel of residual). Not with the roll for the same camera (refused): about the line of sight they are the same.
- The maplets' offsets, held by `offset_sigma`, keep the body's frame: every image turned one way and every maplet
  the other would explain the shifts alike.
- For the caller, the turn applies to the camera the other way about the origin: position `R.T @ p`, axes `A @ R`,
  `R` the rotation of the vector (then measured again: the lever is first order).
- Python: `pointing(..., turn=None)`. The first result gains three columns, the turn's vector (docs/API.md).

Tests:
- `a_turn_is_the_body_turned_about_its_origin`: one camera, thirty maplets over the face it sees, offsets held, the
  body turned by a known rotation of a few hundredths of a degree: given back within 1e-6 rad, no shift.
- `the_shifts_give_back_each_images_turn`: 60 cameras about 300 maplets, half seeing the body turned by up to three
  degrees about any axis, the shifts made with the maplets turned (not the linear lever), 0.2 px of noise, one in
  twenty a wrong lock. The turns come back within 0.05° about the line of sight and 0.2° across it, pointings within a
  pixel. Across the line of sight a turn moves the face seen almost as a shift does, by each maplet's depth: only the
  maplets towards the limb tell the two apart, and the shift takes what the turn leaves. More iterations do not
  change it (40 to 640: the same).
- `a_turn_and_a_roll_together_are_refused`.
- `tests/test_shape.py`: the same through Python, worst 0.074° across, 0.014° along.

Open: SPC's rounds applying each image's turn and solving again (the caller's loop), and a rotation model tying the
images of a session together instead of a free turn each.

## Later the same day: one turn and its rate a session

An image registered in a handful of maplets cannot tell three angles of turn from its shift (each one's own turn
takes the noise). A body turns smoothly, though, so the images of one session can share a turn and its rate:

- `d[k] = a[g] + (t[k] - t[g]) b[g]`, `t[g]` the session's mean time: `group` (a number a camera, -1 none) and
  `time` (hours) say which images share one, each keeping its own shift. A session's shifts, turn and rate are
  fitted at once from all its registrations (2n + 6 unknowns, a dense solve), the rate held weakly as the turn.
- A turn's wrong locks: past three Huber lengths or three times the median error of the registrations it is fitted
  with, whichever is more (a turn of degrees not yet fitted moves every maplet by tens of pixels, none wrong). The
  roll's rule is unchanged.
- Python: `pointing(..., turn=..., group=..., time=...)`.

Tests (`a_session_shares_one_turn_and_its_rate`): six sessions of ten images over two hours, the view sweeping 60°
about the pole in each, each session's body turned by up to 3° about each axis and turning at up to 3°/h, eight
registrations an image, 0.2 px of noise and one in twenty a wrong lock, three rounds as SPC applies each turn and
measures again. Each image alone: 2.1° off at worst across the line of sight, 0.6° about it; a session's model 0.7°
and 0.4°. Without the wrong locks 0.2° and 0.05° (alone 0.3° and 0.08°); without noise either, 0.01°: the rounds find
the truth. With a session seen from one direction only, the turn across the line of sight stays hard (each image's
shift takes it): the sweep of the view is what tells it. `tests/test_shape.py`: ten registrations an image, worst
0.089° shared against 0.166° alone.
