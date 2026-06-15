from util import helper


class Service:
    def run(self) -> str:
        return helper("world")


def main() -> str:
    service = Service()
    return service.run()
