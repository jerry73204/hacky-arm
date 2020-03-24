#!/usr/bin/env python3
import cv2 as cv
import argparse
from detector import Detector
from video import Video
from arm import Arm
import json



parser = argparse.ArgumentParser()
parser.add_argument(
    '--config',
    # default=None,
    # default='output.json',
    default='configs/0325.json',
    help='load config file'
)
parser.add_argument(
    '--input',
    # default='./demo.jpg',
    # default='/dev/video6',
    # default='/dev/video6',
    default='realsense',
    help='input source'
)
parser.add_argument(
    '--data',
    default='data.csv',
    help='data recording the pair of arm/target position'
)
args = parser.parse_args()


WIDTH = 640
HEIGHT = 480
FONT_SCALE = 0.5
FONT_THICKNESS = 1

# WIDTH = 1280
# HEIGHT = 720

video = Video(args.input, width=WIDTH, height=HEIGHT)

if args.config is not None:
    with open(args.config) as f:
        config = json.load(f)
    detector = Detector(**config)
else:
    detector = Detector()


def do_nothing(x):
    return None


# tunable panel
panel = 'panel'
cv.namedWindow(panel)
cv.moveWindow(panel, 1020, 60)
cv.createTrackbar('lh', panel, detector.cfg['lower_bound'][0], 255, do_nothing)
cv.createTrackbar('uh', panel, detector.cfg['upper_bound'][0], 255, do_nothing)
cv.createTrackbar('ls', panel, detector.cfg['lower_bound'][1], 255, do_nothing)
cv.createTrackbar('us', panel, detector.cfg['upper_bound'][1], 255, do_nothing)
cv.createTrackbar('lv', panel, detector.cfg['lower_bound'][2], 255, do_nothing)
cv.createTrackbar('uv', panel, detector.cfg['upper_bound'][2], 255, do_nothing)
cv.createTrackbar('inversion', panel, int(detector.cfg['inversion']), 1, do_nothing)
for key in ['blur_kernel', 'dilation_kernel', 'erosion_kernel']:
    cv.createTrackbar(key, panel, detector.cfg[key], 41, do_nothing)
for key in ['n_dilations', 'n_erosions']:
    cv.createTrackbar(key, panel, detector.cfg[key], 20, do_nothing)
cv.createTrackbar(
    'n_objects',
    panel,
    detector.cfg['n_objects'],
    10,
    do_nothing
)
for key in ['min_arc_length', 'max_arc_length']:
    cv.createTrackbar(
        key,
        panel,
        detector.cfg[key],
        2 * detector.cfg[key],
        do_nothing
    )
cv.createTrackbar(
    'roi_width',
    panel,
    int(detector.cfg['roi'][0] * 100),
    100,
    do_nothing
)
cv.createTrackbar(
    'roi_height',
    panel,
    int(detector.cfg['roi'][1] * 100),
    100,
    do_nothing
)


arm = Arm()

window = 'window'
cv.namedWindow(window)
cv.moveWindow(window, 360, 60)

collecting = False
data_pair = []

arm.go_home()

try:
    while True:
        raw = video.get_frame()

        # for cnt in contours:
        mid, img, objects = detector.detect(raw)

        # show arm position
        arm_pos = arm.position[:3]
        arm_pos = tuple(int(p) for p in arm_pos)
        cv.putText(
            img,
            f'End effector: {arm_pos}',
            (5, int(HEIGHT * 0.9)),
            cv.FONT_HERSHEY_SIMPLEX,
            FONT_SCALE,
            (0, 255, 215),
            FONT_THICKNESS,
        )

        # show target position
        if len(objects) > 0:
            obj_pos = objects[0]['point']
            cv.putText(
                img,
                f'Target: {obj_pos}',
                (5, int(HEIGHT * 0.95)),
                cv.FONT_HERSHEY_SIMPLEX,
                FONT_SCALE,
                (0, 255, 215),
                FONT_THICKNESS,
            )
        else:
            cv.putText(
                img,
                f'Target: none',
                (5, int(HEIGHT * 0.95)),
                cv.FONT_HERSHEY_SIMPLEX,
                FONT_SCALE,
                (0, 255, 215),
                FONT_THICKNESS,
            )

        if len(data_pair) > 0:
            if len(data_pair) == 2:
                cv.putText(
                    img,
                    f'Target: {data_pair}',
                    (int(WIDTH * 0.7), int(HEIGHT * 0.9)),
                    cv.FONT_HERSHEY_SIMPLEX,
                    FONT_SCALE,
                    (0, 255, 215),
                    FONT_THICKNESS,
                )
            else:
                cv.putText(
                    img,
                    f'Data: {data_pair} saved',
                    (int(WIDTH * 0.55), int(HEIGHT * 0.9)),
                    cv.FONT_HERSHEY_SIMPLEX,
                    FONT_SCALE,
                    (0, 0, 215),
                    FONT_THICKNESS,
                )

        # display collecting information
        if not collecting:
            info = 'Press <space> to record the target position.'
        else:
            info = 'Move arm and press <sapce> to record end-effector position.'
        cv.putText(
            img,
            info,
            (5, 40),
            cv.FONT_HERSHEY_SIMPLEX,
            FONT_SCALE,
            (0, 255, 215),
            FONT_THICKNESS,
        )

        cv.imshow(window, img)
        cv.imshow(panel, mid)
        key = cv.waitKey(1)

        # q for quit
        if key == 113:
            if collecting:
                collecting = not collecting
            else:
                break

        # s for saving the configuration
        elif key == 115:
            detector.save('output.json')

        # h for setting home
        elif key == 104:
            arm.go_home()

        # r for resetting home
        elif key == 114:
            arm.reset_home()

        # <space> for collecting data
        elif key == 32:
            if collecting:
                data_pair += arm_pos
                assert len(data_pair) == 5
                data_pair = ','.join([str(d) for d in data_pair])
                with open(args.data, 'a') as f:
                    f.write(data_pair)
                    f.write('\n')
                arm.go_home()
            else:
                data_pair = [*obj_pos]
            collecting = not collecting

        else:
            for param in [
                'blur_kernel',
                'n_dilations',
                'dilation_kernel',
                'erosion_kernel',
                'n_erosions',
                'n_objects',
                'min_arc_length',
                'max_arc_length',
            ]:
                detector.cfg[param] = cv.getTrackbarPos(param, panel)

            detector.cfg['inversion'] = bool(
                cv.getTrackbarPos('inversion', panel)
            )
            detector.cfg['roi'] = (
                cv.getTrackbarPos('roi_width', panel) / 100.0,
                cv.getTrackbarPos('roi_height', panel) / 100.0,
            )
            detector.cfg['lower_bound'] = (
                cv.getTrackbarPos('lh', panel),
                cv.getTrackbarPos('ls', panel),
                cv.getTrackbarPos('lv', panel),
            )
            detector.cfg['upper_bound'] = (
                cv.getTrackbarPos('uh', panel),
                cv.getTrackbarPos('us', panel),
                cv.getTrackbarPos('uv', panel),
            )

except KeyboardInterrupt:
    print('Terminated.')

cv.destroyAllWindows()
video.close()
