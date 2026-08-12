"""Read-only, deterministic projections of the pinned Python oracle."""

from .projections.choice import choice_projection
from .projections.event import event_projection
from .projections.error import error_projection
from .projections.outcome import outcome_projection
from .projections.state import state_projection
from .projections.view import view_projection

__all__ = [
    "choice_projection",
    "event_projection",
    "error_projection",
    "outcome_projection",
    "state_projection",
    "view_projection",
]
