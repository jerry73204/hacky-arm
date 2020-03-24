import pyrealsense2 as rs
import numpy as np
import cv2 as cv


class Video:

    def __init__(self, source, width=640, height=480):
        self.width = width
        self.height = height

        if '/dev/video' in source:
            self.source = 'camera'
            self.camera = cv.VideoCapture(int(source.split('/dev/video')[-1]))

        elif source == 'realsense':
            self.source = 'realsense'
            self.realsense = rs.pipeline()
            config = rs.config()
            config.enable_stream(
                rs.stream.color,
                width,
                height,
                rs.format.bgr8,
                # rs.format.rgb8,
                30
            )
            self.realsense.start()
        else:
            self.source = 'image'
            raw_img = cv.imread(source)
            self.frame = cv.resize(raw_img, (width, height))

    def get_frame(self):
        if self.source == 'camera':
            _, frame = self.camera.read()
            self.frame = cv.resize(frame, (self.width, self.height))

        elif self.source == 'realsense':
            frames = self.realsense.wait_for_frames()
            color_frame = frames.get_color_frame()
            if color_frame:
                self.frame = np.asanyarray(color_frame.get_data())
                self.frame = cv.cvtColor(self.frame, cv.COLOR_BGR2RGB)
            else:
                self.frame = self.frame

        return self.frame

    def close(self):
        if self.source == 'camera':
            self.camera.release()
        elif self.source == 'realsense':
            self.realsense.stop()
