import argparse
import cv2
import numpy as np
import tensorflow as tf
tf.compat.v1.disable_v2_behavior()
p=argparse.ArgumentParser(); p.add_argument("input"); p.add_argument("output"); a=p.parse_args()
line=cv2.imread(a.input, cv2.IMREAD_GRAYSCALE)
line=cv2.resize(line,(320,320)); _,line=cv2.threshold(line,200,255,cv2.THRESH_BINARY)
ink=line.astype(np.float32)[None,:,:,None]/255.0
g=tf.compat.v1.GraphDef()
with open("training/shading/shadesketch/models/linenorm.pb","rb") as f: g.ParseFromString(f.read())
tf.import_graph_def(g,name="lineNorm")
with tf.compat.v1.Session() as s:
    n=s.run("lineNorm/conv2d_9/Sigmoid:0", {"lineNorm/input_1:0": ink})
n=np.squeeze(n)
print(n.shape,n.min(),n.max())
if n.shape[-1]>=3:
    img=(n[...,:3]*255).astype(np.uint8)
else:
    img=(np.repeat(n[...,:1],3,axis=-1)*255).astype(np.uint8)
cv2.imwrite(a.output,cv2.cvtColor(img,cv2.COLOR_RGB2BGR))
