"""Read-only, deterministic projections of the pinned Python oracle."""

from .projections.state import state_projection
from .projections.view import view_projection

__all__ = ["state_projection", "view_projection"]
