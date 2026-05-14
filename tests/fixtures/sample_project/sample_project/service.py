class SampleService:
    """Coordinates sample work."""

    def run(self, name: str) -> str:
        return helper(name)

def helper(name: str) -> str:
    """Build a friendly response."""
    return f"hello {name}".strip()
