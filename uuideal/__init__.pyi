from uuid import UUID

__all__ = [
    "install",
    "uninstall",
    "installed",
    "uuid1",
    "uuid3",
    "uuid4",
    "uuid5",
    "uuid6",
    "uuid7",
    "uuid8",
    "reseed_rng",
]

def install() -> None:
    """Install vectorcall patches that accelerate stdlib uuid functions and UUID methods"""

def uninstall() -> None:
    """Uninstall vectorcall patches and restore stdlib uuid function and UUID method behavior"""

def installed() -> bool:
    """Return whether uuideal vectorcall patches are currently installed"""

def uuid1(node: int | None = None, clock_seq: int | None = None) -> UUID:
    """Generate a version 1 UUID without installing global uuid module patches"""

def uuid3(namespace: UUID, name: str | bytes) -> UUID:
    """Generate a version 3 UUID without installing global uuid module patches"""

def uuid4() -> UUID:
    """Generate a version 4 UUID without installing global uuid module patches"""

def uuid5(namespace: UUID, name: str | bytes) -> UUID:
    """Generate a version 5 UUID without installing global uuid module patches"""

def uuid6(node: int | None = None, clock_seq: int | None = None) -> UUID:
    """Generate a version 6 UUID without installing global uuid module patches."""

def uuid7() -> UUID:
    """Generate a version 7 UUID without installing global uuid module patches."""

def uuid8(a: int | None = None, b: int | None = None, c: int | None = None) -> UUID:
    """Generate a version 8 UUID without installing global uuid module patches."""

def reseed_rng() -> None:
    """Reseed the Rust random number generator used by uuideal."""
