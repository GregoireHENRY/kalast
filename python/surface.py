import numpy

import kalast


def update(vertex: kalast.Vertex) -> kalast.Vertex:
    vertex.material = kalast.Material(
        albedo=0.1,
        emissivity=0.9,
        thermal_inertia=500.0,
        density=2100.0,
        heat_capacity=600.0,
    )
    vertex.color *= 1.0 - vertex.material.albedo
    return vertex


if __name__ == "__main__":
    shape = kalast.IntegratedShapeModel.Cube
    surf = kalast.Surface.use_integrated(shape)

    surf.update_all(update)

    print(surf)
    print(surf.vertices)
