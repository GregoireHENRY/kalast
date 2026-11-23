# 2026-11-04 — `kalast.shape.pointing`: an image's roll, fitted with its pointing

An image whose camera attitude is only roughly known (a planned attitude, before the measured one is delivered) is
off about its boresight too: its roll turns every maplet about the image's middle, a shift that grows with the
distance from it. `pointing` fitted each image's pointing as a shift only, so such images could not be used.

What changed (`src/shape/pointing.rs`):

- `s[k, t] = P[k] + R[k] (q.y, -q.x) + J[k, t] o[t]`: `q` the maplet's pixel from the image's middle, `R[k]` the
  roll in radians, what turning the camera's axes about their `z` by it does (`Rz(R) @ axes`, rows `x`, `y`).
- Freed image by image (`roll`, a flag a camera). Those images' shift and roll are fitted by weighted least squares
  from the Huber loss's weights, with a weak hold on the roll (0.1 rad for a pixel of residual) for images whose
  maplets all lie near their middle. The images not freed keep the median, as before, and a roll of zero.
- Wrong locks are left out of a freed image's fit: past three Huber lengths, a shift weighs nothing there. Under the
  Huber weight alone, a wrong lock far from the middle turned an image by hundredths of a degree (0.069° worst in
  the test below, 0.054° once left out).
- Python: `pointing(..., roll=None)`. With `roll`, the first result is `(n, 3)`, its third column the roll, applied
  as `Rz(roll) @ axes` before the shift (docs/API.md).

Tests:
- `a_roll_is_the_axes_turned_about_their_z`: the lever is what turning the axes does, to first order.
- `the_shifts_give_back_each_images_roll`: 60 cameras about 300 maplets. Half are turned by up to two degrees, every
  shift made with the axes turned (not the linear lever), and one in twenty is a wrong lock. Rolls come back within
  0.06° (0.013° at the median), pointings within half a pixel, and the cameras not freed keep zero.
- `tests/test_shape.py`: the same through Python, worst 0.020°.

Open: a camera's position is not fitted. Where a body turns other than as its frame says, a roll can mend only the
part about the line of sight; the rest needs each image's view of the body solved, as SPC does with positions.
