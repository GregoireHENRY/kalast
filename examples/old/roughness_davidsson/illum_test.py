#!/usr/bin/env python

import numpy  # noqa
import glm

import kalast


def closure_update_state(
    state: kalast.gpu.win.StateStep,
) -> kalast.gpu.win.StateStep | None:
    if state.iteration == 0:
        rot = glm.mat3(
            glm.rotate(
                -90.0 * kalast.util.RPD,
                [1.0, 0.0, 0.0],
            )
        )
    else:
        rot = glm.mat3(
            glm.rotate(
                -100.0 * kalast.util.RPD * state.dt.total_seconds(), [0.0, 1.0, 0.0]
            )
        )

    # state.light_pos = state.light_pos @ rot

    for iib in range(0, len(state.models_state)):
        model = state.get_model_state(iib)
        # model.p[0] = 1.0

        # model.m = model.m @ rot

        state.set_model_state(iib, model)

    return state


config = kalast.gpu.config.Config()
# config.debug_event_device = True
# config.debug_event_window_except_redraw = True
config.width = 1440
config.height = 1080
config.render_light = True
config.background = [0.0, 0.0, 0.0, 1.0]
config.enable_back_face = True

config.camera_pos = [10.0, 0.0, 0.0]
config.camera_dir = [-1.0, 0.0, 0.0]
config.camera_up = [0.0, 0.0, 1.0]

config.camera_fovy = 45.0
config.camera_znear = 0.1
config.camera_zfar = 100.0
config.camera_speed = 4.0
config.camera_sensitivity = 0.4

config.light_pos = [10.0, 0.0, 0.0]
config.light_color = [1.0, 1.0, 1.0]

config.start_paused = True
config.ambient_strength = 0.1

config.models = [
    kalast.gpu.config.ConfigModel(
        path="../res/plane_crater_1024-512_h=0.437.obj",
        flat=True,
        # color_mode=1,
        # color=[1.0, 1.0, 1.0]
    ),
]

kalast.gpu.win.run(config, closure_update_state)
