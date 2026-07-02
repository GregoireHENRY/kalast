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

        model.m = model.m @ rot

        state.set_model_state(iib, model)

    return state


config = kalast.gpu.config.Config()
config.debug_state_creation = False
config.debug_state_rendering = False
config.width = 1440
config.height = 1080
config.depthpass_enable = False
config.render_light = True
config.hdr_enable = False
config.background = [0.0, 0.0, 0.0, 1.0]
config.texts = [
    # kalast.gpu.config.ConfigText("Hello custom text", pos=[1440.0, 0.0], ha="right"),
]
config.show_text_info = True
config.fps_time_refresh = 0.05

config.camera_pos = [0.0, 5.0, 10.0]
config.camera_yaw = -90.0
config.camera_pitch = -20.0
config.light_pos = [0.0, 0.0, 4.0]

# config.camera_pos = [0.0, -10.0, 0.0]
# config.camera_yaw = 90.0
# config.camera_pitch = 90.0
# config.light_pos = [0.0, -4.0, 0.0]

config.camera_fovy = 45.0
config.camera_znear = 0.1
config.camera_zfar = 100.0
config.camera_speed = 4.0
config.camera_sensitivity = 0.4
config.light_color = [1.0, 1.0, 1.0]

config.start_paused = True
config.global_test = 0
config.ambient_strength = 0.01
config.diffuse_enable = True
config.specular_enable = False

config.models = [
    kalast.gpu.config.ConfigModel(
        # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimate_1k.obj",
        path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_3072.obj",
        # path="/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003.obj",
        # path="/Users/gregoireh/data/mesh/didymos/didymos_g_1165mm_spc_obj_0000n00000_v003.obj",
        flat=True,
        # pos_factor=[1.0, 1.0, 1.0],
        # color_mode=1,
        # color=[1.0, 1.0, 1.0]
    ),
]

kalast.gpu.win.run(config, closure_update_state)
