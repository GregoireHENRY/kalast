#!/usr/bin/env python

import random

import numpy
import pandas

from kalast.app import App
from kalast.mesh import Mesh


def build_matrix(df, row):
    val = df.iloc[row, 7:16].to_numpy(dtype=float)
    return val.reshape(3, 3)


app = App()
app.config.width = 1020
app.config.height = 1020
app.config.panels_folded = True
app.simulation.config.axes.style = "off"
app.simulation.state.pause_after_iteration = None
# app.simulation.config.light.ambient = 0.05

position_body = "examples/landmark_tracking/synthetic_spacecraft_data_data4kalast_ddid2555_ddim1700_rab1.2_rbc1.4_Stable.csv"
position_camera = "examples/landmark_tracking/camera_positions.txt"

csv_body = pandas.read_csv(position_body)
cam_arr = numpy.loadtxt(position_camera)
nrows = min(csv_body.shape[0], cam_arr.shape[0])
print(f"nrows={nrows}")

sun0 = csv_body.iloc[0, 1:4].to_numpy(dtype=float)
cam0 = cam_arr[0, 1:4]
cos = numpy.dot(sun0, cam0) / (numpy.linalg.norm(sun0) * numpy.linalg.norm(cam0))
print(f"sun-camera cosine: {cos}")

n_points = 3000

id_frame = []
elapsed_s = []
id_facet = []
pos_x = []
pos_y = []
pos_z = []
pos_x_screen = []
pos_y_screen = []
cos_v = []
saved = False

cam0 = cam_arr[0, 1:4] / 1000.0
app.simulation.camera.pos = cam0
app.simulation.camera.dir = -cam0 / numpy.linalg.norm(cam0)
app.simulation.camera.up = [0.0, 0.0, 1.0]
app.simulation.camera.projection.fovy = numpy.deg2rad(5.5)

mesh = Mesh(path="examples/landmark_tracking/dimo_as-bennu_alain.obj")
mesh.flatten()
mesh.colors[:, :3] = 0.15

indices_facets = random.sample(range(len(mesh.facets)), n_points)

pos_bf = []
nor_bf = []
for k in indices_facets:
    pos_bf.append(mesh.facets[k].pos)
    nor_bf.append(mesh.facets[k].normal)
    mesh.colors[k, :] = [1.0, 0.0, 0.0]
    mesh.color_modes[indices_facets] = 1
pos_bf = numpy.array(pos_bf)
nor_bf = numpy.array(nor_bf)

app.simulation.add_mesh(mesh)

n = len(mesh.facets)
print("n_facets:", n, "n_colors:", mesh.colors.shape)
print("unique facet ids:", len(set(indices_facets)))

print("number of facets:", len(mesh.facets))
print("frames to be simulated:", nrows)

print("started")


while app.running:
    it = app.simulation.state.iteration

    if app.simulation.state.is_paused:
        app.step()
        continue

    if it >= nrows:
        if not saved:
            pandas.DataFrame(
                {
                    "frame": id_frame,
                    "elapsed": elapsed_s,
                    "facet_id": id_facet,
                    "x": pos_x,
                    "y": pos_y,
                    "z": pos_z,
                    "x_screen": pos_x_screen,
                    "y_screen": pos_y_screen,
                    "cos_view": cos_v,
                }
            ).to_csv(
                "examples/landmark_tracking/track.csv",
                index=False,
            )
            print(f"saved track.csv: {len(id_frame)} rows")
            saved = True
            app.simulation.state.toggle_pause()
            continue

    r = build_matrix(csv_body, it)
    m = numpy.eye(4)
    m[0:3, 0:3] = r
    app.simulation.bodies[0].mat = m

    app.simulation.sun.pos = csv_body.iloc[it, 1:4].to_numpy(dtype=float) / 1000.0

    cam = cam_arr[it, 1:4] / 1000.0
    app.simulation.camera.pos = cam
    app.simulation.camera.dir = -cam / numpy.linalg.norm(cam)

    app.simulation.export_once()
    app.step()

    elapsed = float(csv_body.iloc[it, 0])
    p_all = pos_bf @ r.T
    n_all = nor_bf @ r.T
    view = cam - p_all
    view /= numpy.linalg.norm(view, axis=1, keepdims=True)
    cos_view = (n_all * view).sum(axis=1)

    for j, k in enumerate(indices_facets):
        id_frame.append(it)
        elapsed_s.append(elapsed)
        id_facet.append(k)
        pos_x.append(p_all[j, 0])
        pos_y.append(p_all[j, 1])
        pos_z.append(p_all[j, 2])
        cos_v.append(cos_view[j])

    for j, k in enumerate(indices_facets):
        xy = app.simulation.project_facet(0, k)
        if xy is None:
            pos_x_screen.append("")
            pos_y_screen.append("")
        else:
            pos_x_screen.append(xy[0])
            pos_y_screen.append(xy[1])


print("finished")
