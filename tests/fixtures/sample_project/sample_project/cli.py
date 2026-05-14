from .service import SampleService

def main() -> int:
    service = SampleService()
    print(service.run("world"))
    return 0
