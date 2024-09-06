from pathlib import Path

import arrow
import numpy


def round_date(date: str) -> arrow.Arrow:
    """utc"""
    return arrow.get(arrow.get(date).date())


def round_date_as_str(date: str) -> str:
    """utc"""
    return round_date(date).format("YYYY-MM-DDTHH:mm:ss")


def elapsed_round_day(date: str) -> float:
    time = arrow.get(date)
    time_round_day = round_date(date)
    return time - time_round_day


def is_new_day_for_reading(date: str) -> bool:
    return elapsed_round_day(date) > 0


def index_to_take(date: str, step_time: float) -> int:
    return int(elapsed_round_day(date).seconds / step_time)


def read_binary_temperature(path: Path | str, shape: tuple[int, int] | None = None):
    if isinstance(path, str):
        path = Path(path)

    # Read binary file.
    # shape can be: (-1, NUMBER_FACETS) if you don't know the size of time.
    # Temperatures are stored as uint16 centi-kelvins. Convert it back to kelvins as floats.
    M = numpy.fromfile(path, dtype=numpy.uint16)

    if shape is not None:
        M = M.reshape((shape))

    return M.astype(numpy.float64) / 100.0
