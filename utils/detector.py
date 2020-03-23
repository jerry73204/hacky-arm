import cv2 as cv
import numpy as np
import json


def to_odd(x):
    if not isinstance(x, int):
        x = int(x)
    if x < 3:
        return 3
    elif x % 2 == 0:
        return x + 1
    else:
        return x


class Detector:

    def __init__(
        self,
        inversion=False,
        blur_kernel=23,
        n_dilations=3,
        dilation_kernel=3,
        n_erosions=3,
        erosion_kernel=3,
        n_objects=5,
        min_arc_length=100,
        max_arc_length=1500,
        roi=(0.8, 0.8),
        lower_bound=(7, 50, 63),
        upper_bound=(21, 155, 255),
    ):
        self.cfg = {
            'inversion': inversion,
            'blur_kernel': blur_kernel,
            'n_dilations': n_dilations,
            'dilation_kernel': dilation_kernel,
            'n_erosions': n_erosions,
            'erosion_kernel': erosion_kernel,
            'n_objects': n_objects,
            'min_arc_length': min_arc_length,
            'max_arc_length': max_arc_length,
            'roi': roi,
            'lower_bound': lower_bound,
            'upper_bound': upper_bound,
        }

    def save(self, json5_file):
        with open(json5_file, 'w') as f:
            json.dump(self.cfg, f, indent=4)

    def detect(self, raw):

        # HSV threshold
        img = cv.inRange(
            cv.cvtColor(raw, cv.COLOR_BGR2HSV),
            np.array(self.cfg['lower_bound']),
            np.array(self.cfg['upper_bound'])
        )

        img = cv.medianBlur(img, to_odd(self.cfg['blur_kernel']))

        if self.cfg['inversion']:
            img = 255 - img

        # dilation
        dilation_kernel = (
            to_odd(self.cfg['dilation_kernel']),
            to_odd(self.cfg['dilation_kernel'])
        )
        img = cv.dilate(
            img,
            dilation_kernel,
            iterations=self.cfg['n_dilations']
        )

        # erosion
        erosion_kernel = (
            to_odd(self.cfg['erosion_kernel']),
            to_odd(self.cfg['erosion_kernel']),
        )
        img = cv.erode(
            img,
            erosion_kernel,
            iterations=self.cfg['n_erosions']
        )

        # before finding contours
        medium = img

        # find contours
        contours, hierarchy = cv.findContours(
            img,
            cv.RETR_EXTERNAL,
            cv.CHAIN_APPROX_NONE
        )
        contours = sorted(
            contours,
            key=lambda cnt: cv.arcLength(cnt, True),
            reverse=True
        )

        # region of interest
        height, width = raw.shape[:2]
        center_x = width // 2
        center_y = height // 2
        shift_x = int(width * self.cfg['roi'][0] / 2)
        shift_y = int(height * self.cfg['roi'][1] / 2)
        roi_point_1 = (center_x - shift_x, center_y - shift_y)
        roi_point_2 = (center_x + shift_x, center_y + shift_y)
        cv.rectangle(raw, roi_point_1, roi_point_2, (255, 0, 0), 2)

        objects = []
        for cnt in contours[:self.cfg['n_objects']]:
            rotated_rect = cv.minAreaRect(cnt)
            arc_len = cv.arcLength(cnt, True)

            # check arc length bound
            out_of_arc_length = arc_len < self.cfg['min_arc_length'] \
                or arc_len > self.cfg['max_arc_length']
            if out_of_arc_length:
                continue

            # get point and angle of the enclosing rectangle
            point = tuple(int(x) for x in rotated_rect[0])
            angle = rotated_rect[2]

            # check if it's in the region of interest
            out_of_roi = False
            for i in range(2):
                if point[i] < roi_point_1[i] or point[i] > roi_point_2[i]:
                    out_of_roi = True
                    break
            if out_of_roi:
                continue

            # collect object and print the info
            objects.append({'point': point, 'angle': angle})
            cv.putText(
                raw,
                f'{point}',
                (point[0] + 50, point[1] - 10),
                cv.FONT_HERSHEY_SIMPLEX,
                0.5,
                (0, 0, 255),
                2,
            )
            cv.putText(
                raw,
                f'{angle:.2f}',
                (point[0] + 50, point[1] + 15),
                cv.FONT_HERSHEY_SIMPLEX,
                0.5,
                (0, 0, 255),
                2,
            )

            # draw the bounding box
            box = cv.boxPoints(rotated_rect)
            box = np.int0(box)
            cv.drawContours(raw, [box], 0, (0, 255, 0), 2)

        return medium, raw, objects
