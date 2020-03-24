from serial.tools import list_ports
from dobot import Dobot
import time


class Arm:

    def __init__(
        self,
        home=(220.0, 0.0, 135.0, 9.0),
        port=None,
        verbose=False
    ):
        assert len(home) == 4
        self.home = home
        if port is None:
            port = list_ports.comports()[0].device
        self.device = Dobot(port=port, verbose=verbose)

    @property
    def position(self):
        return self.device.pose()[:4]

    def go_home(self):
        self.device.wait_for_cmd(self.device.move_to(*self.home))

    def reset_home(self):
        self.device.wait_for_cmd(self.device.home())
        self.go_home()

    def grip(self, toggle: bool):
        self.device.wait_for_cmd(self.device.grip(toggle))

    def grab(self, x, y, angle, deep=False):
        self.grip(False)

        # horizontal move to target
        self.device.wait_for_cmd(
            self.device.move_to(x, y, self.home[2], self.home[3])
        )

        # go down
        if deep:
            self.device.wait_for_cmd(
                self.device.move_to(x, y, -25, self.home[3])
            )
        else:
            self.device.wait_for_cmd(
                self.device.move_to(x, y, -25, self.home[3])
            )
        # and grip
        self.grip(True)

        time.sleep(1)
        self.go_home()
        self.grip(False)
