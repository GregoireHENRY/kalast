import numpy  # noqa

import kalast


config = kalast.gpu.config.Config()
config.debug_state_creation = False
config.debug_state_creation = False
config.width = 1440
config.height = 1080
config.depthpass_enable = False
config.render_light = True
config.hdr_enable = False
config.background = [0.0, 0.0, 0.0, 1.0]
config.camera_pos = [0.0, 5.0, 10.0]
config.camera_yaw = -90.0
config.camera_pitch = -20.0
config.camera_fovy = 45.0
config.camera_znear = 0.1
config.camera_zfar = 100.0
config.camera_speed = 4.0
config.camera_sensitivity = 0.4
config.light_pos = [0.0, 0.0, 4.0]
config.light_color = [1.0, 1.0, 1.0]
config.start_paused = True
config.global_test = 0
config.ambient_strength = 0.005
config.diffuse_enable = True
config.specular_enable = False
config.instances_count_per_row = 1
config.instances_space_between = 3.0
config.instances_displacement = [1.5, 0.0, 1.5]
config.models = [
    kalast.gpu.config.ConfigModel(
        path="../res/ico3.obj",
        # flat=True,
        # pos_factor=[1.0, 1.0, 1.0],
        # color_mode=1,
        # color=[1.0, 1.0, 1.0]
    ),
]

kalast.gpu.win.run(config)
