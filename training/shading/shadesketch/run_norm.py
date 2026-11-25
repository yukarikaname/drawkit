import argparse, cv2, numpy as np, tensorflow as tf
tf.compat.v1.disable_v2_behavior()
p=argparse.ArgumentParser(); p.add_argument("input"); p.add_argument("output"); p.add_argument("--direction", default="-1,1,-1"); a=p.parse_args()
line=cv2.imread(a.input, cv2.IMREAD_GRAYSCALE)
line=cv2.resize(line,(320,320)); _,line=cv2.threshold(line,200,255,cv2.THRESH_BINARY)
ink=line.astype(np.float32)[None,:,:,None]/255.0
ng=tf.compat.v1.GraphDef()
with open("training/shading/shadesketch/models/linenorm.pb","rb") as f: ng.ParseFromString(f.read())
tf.import_graph_def(ng,name="lineNorm")
sg=tf.compat.v1.GraphDef()
with open("training/shading/shadesketch/models/lineshader.pb","rb") as f: sg.ParseFromString(f.read())
tf.import_graph_def(sg,name="lineShader")
direction=np.array([[float(v) for v in a.direction.split(",")]],dtype=np.float32)
with tf.compat.v1.Session() as s:
    norm=s.run("lineNorm/conv2d_9/Sigmoid:0", {"lineNorm/input_1:0": ink})
    raw=s.run("lineShader/conv2d_139/Tanh:0", {"lineShader/input_1:0": direction, "lineShader/input_2:0": 1.0-norm})
raw=np.squeeze(raw); shadow=np.clip((1.0-(raw+1.0)/2.0),0,1)
cv2.imwrite(a.output,(shadow*255).astype(np.uint8))
paper=np.ones((*shadow.shape,3),np.float32); paper*=1-np.squeeze(norm)[...,None]
paper=paper*(1-shadow[...,None]*0.6)+np.array([0.42,0.36,0.62],np.float32)*shadow[...,None]*0.6
cv2.imwrite(a.output.replace('.png','_comp.png'),cv2.cvtColor(np.clip(paper*255,0,255).astype(np.uint8),cv2.COLOR_RGB2BGR))
print("shadow mean",shadow.mean(),"norm range",norm.min(),norm.max())
