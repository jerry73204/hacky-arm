#!/usr/bin/env python3
import cv2 as cv
import argparse
from detector import Detector
import json

parser = argparse.ArgumentParser()
parser.add_argument(
    '--config',
    default=None,
    help='load config file'
)
parser.add_argument(
    '--input',
    default='./demo.jpg',
    help='input source'
)
args = parser.parse_args()


WIDTH = 640
HEIGHT = 480

if '/dev/video' in args.input:
    use_camera = True
    cap = cv.VideoCapture(int(args.input.split('/dev/video')[-1]))
else:
    use_camera = False
    frame = cv.imread(args.input)


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

window = 'window'
cv.namedWindow(window)
cv.moveWindow(window, 360, 60)

while True:
    if use_camera:
        ret, frame = cap.read()
    raw = cv.resize(frame, (WIDTH, HEIGHT))

    # for cnt in contours:
    mid, img, objects = detector.detect(raw)
    cv.imshow(window, img)
    cv.imshow(panel, mid)
    key = cv.waitKey(1)
    if key == 113:
        break
    elif key == 13:
        detector.save('output.json')
    else:
        for key in [
            'blur_kernel',
            'n_dilations',
            'dilation_kernel',
            'erosion_kernel',
            'n_erosions',
            'n_objects',
            'min_arc_length',
            'max_arc_length',
        ]:
            detector.cfg[key] = cv.getTrackbarPos(key, panel)

        detector.cfg['inversion'] = bool(cv.getTrackbarPos('inversion', panel))
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

cv.destroyAllWindows()
if use_camera:
    cap.release()
