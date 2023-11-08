import numpy

import kalast

if __name__ == "__main__":
    shape = kalast.IntegratedShapeModel.Cube
    surf = kalast.RawSurface.use_integrated(shape)

    print(surf)
    print(surf.positions)

    surf.update_all(lambda position: position + 1)

    print(surf.positions)
