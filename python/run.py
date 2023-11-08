import glm
import numpy

import kalast

PERIOD_SPIN_DIDYMOS = 2.26 * 3600.0
AXIS_SPIN_DIDYMOS = numpy.array([0.0, 0.0, -1.0], dtype=numpy.float32)


def update_vertices(vertex: kalast.Vertex) -> kalast.Vertex:
    vertex.material = kalast.Material(
        albedo=0.1,
        emissivity=0.9,
        thermal_inertia=500.0,
        density=2100.0,
        heat_capacity=600.0,
    )
    vertex.color *= 1.0 - vertex.material.albedo
    return vertex


def render(ctx: kalast.Context) -> kalast.Context:
    # print(f"{ctx.iteration}: {ctx.elapsed_time} {ctx.delta_time}");

    angle_spin_didymos = 2.0 * numpy.pi * ctx.delta_time / PERIOD_SPIN_DIDYMOS
    matrix_spin = glm.rotate(angle_spin_didymos, AXIS_SPIN_DIDYMOS)
    ctx.asteroid_matrix_model = numpy.array(
        glm.mat4(ctx.asteroid_matrix_model) * matrix_spin
    )

    return ctx


if __name__ == "__main__":
    surface = (
        kalast.Surface.read_file(
            "../assets/shape models/g_09740mm_spc_obj_didy_0000n00000_v002.obj"
            # "../assets/shape models/g_01220mm_spc_obj_didy_0000n00000_v002.obj"
        )
        .update_all(update_vertices)
        .build()
    )
    interior = [kalast.Interior(0.40, 1e-2, 0.0) for ii in range(len(surface.vertices))]
    record = [kalast.RecordData.these(kalast.RecordDataType.TemperatureSurface, [4287])]

    asteroid = kalast.Asteroid(surface, interior, record)

    sun_position = numpy.array([1.0, 0.0, 0.0], dtype=numpy.float32) * kalast.AU * 1e-3

    simu_settings = kalast.SimulationSettings()
    simu_settings.compute_thermal = True
    simu_settings.simulation = True
    simu_settings.simulation_stop = 51.0 * PERIOD_SPIN_DIDYMOS
    simu_settings.simulation_time_step = 300.0
    simu_settings.record = True
    simu_settings.record_start = simu_settings.simulation_stop - 1.0 * PERIOD_SPIN_DIDYMOS
    simu_settings.record_time_step = 30.0
    simu_settings.record_path = "../assets/records/didymos (new).parquet"
    simu_settings.colormap = kalast.Colormap.Inferno
    simu_settings.colormap_bounds = (0.0, 350.0)

    scene_settings = kalast.SceneSettings()
    scene_settings.camera_position = numpy.array([1.6, 0.0, 0.0], dtype=numpy.float32)
    scene_settings.light_target_offset = 0.5

    win_settings = kalast.WindowSettings()

    win = kalast.Window(win_settings)
    win.mainloop(simu_settings, scene_settings, asteroid, sun_position, render)
