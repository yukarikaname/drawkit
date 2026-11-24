# Train the tiny stroke-shape model used by crates/core/src/shape.rs.
#
#   uv run --with numpy --with scikit-learn python scripts/train_shape_model.py
#
# The script generates noisy lines, ellipses, rectangles and freehand scribbles, extracts the same
# eleven features as shape.rs, trains an 11-16-4 tanh network and writes target/shape_weights.npz.
# It prints the Rust arrays; paste them into the four constants in shape.rs. The feature block here
# and in shape.rs must stay in step or the weights are meaningless.
import math, random
import numpy as np
from sklearn.neural_network import MLPClassifier
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score

random.seed(7); np.random.seed(7)

def resample(points, n=32):
    pts=[]
    for p in points:
        if not pts or math.dist(p, pts[-1]) > 1e-3:
            pts.append(p)
    if len(pts) < 2: return pts
    length=[0.0]
    for i in range(1,len(pts)):
        length.append(length[-1]+math.dist(pts[i-1],pts[i]))
    total=length[-1]
    if total < 1e-6: return pts
    out=[]; j=0
    for k in range(n):
        target=total*k/(n-1)
        while j < len(length)-2 and length[j+1] < target: j+=1
        seg=length[j+1]-length[j]
        t=0 if seg < 1e-9 else (target-length[j])/seg
        out.append((pts[j][0]+(pts[j+1][0]-pts[j][0])*t,
                    pts[j][1]+(pts[j+1][1]-pts[j][1])*t))
    for _ in range(2):
        sm=[out[0]]
        for i in range(1,len(out)-1):
            sm.append(((out[i-1][0]+2*out[i][0]+out[i+1][0])/4,
                       (out[i-1][1]+2*out[i][1]+out[i+1][1])/4))
        sm.append(out[-1]); out=sm
    return out

def features(points):
    q=resample(points)
    if len(q)<4: return None
    xs=[p[0] for p in q]; ys=[p[1] for p in q]
    w=max(xs)-min(xs); h=max(ys)-min(ys); diag=math.hypot(w,h) or 1
    length=sum(math.dist(q[i-1],q[i]) for i in range(1,len(q))) or 1
    closed=1-math.dist(q[0],q[-1])/length
    ax,ay=q[0]; bx,by=q[-1]; dx=bx-ax; dy=by-ay; L=math.hypot(dx,dy) or 1
    maxdev=max(abs((x-ax)*dy-(y-ay)*dx)/L for x,y in q)
    turns=[]; total=0; reversals=0; prev=0
    for i in range(1,len(q)-1):
        v1=(q[i][0]-q[i-1][0],q[i][1]-q[i-1][1]); v2=(q[i+1][0]-q[i][0],q[i+1][1]-q[i][1])
        a1=math.atan2(v1[1],v1[0]); a2=math.atan2(v2[1],v2[0])
        d=(a2-a1+math.pi)%(2*math.pi)-math.pi
        turns.append(d); total+=abs(d)
        if prev and d*prev<0: reversals+=1
        prev=d
    corners=sum(1 for d in turns if abs(d)>math.radians(45))/max(len(turns),1)
    turn=min(total/(2*math.pi),2)
    area=abs(sum(q[i-1][0]*q[i][1]-q[i][0]*q[i-1][1] for i in range(1,len(q)))/2)
    fill=min(area/max(w*h,1e-6),1)
    cx=sum(xs)/len(xs); cy=sum(ys)/len(ys)
    rs=[math.hypot(x-cx,y-cy) for x,y in q]; mr=sum(rs)/len(rs) or 1
    rv=min((sum((r-mr)**2 for r in rs)/len(rs))**.5/mr,2)
    segs=[(q[i][0]-q[i-1][0],q[i][1]-q[i-1][1]) for i in range(1,len(q))]
    aligned=sum(1 for dx,dy in segs if min(abs(math.atan2(dy,dx))%(math.pi/2), math.pi/2-abs(math.atan2(dy,dx))%(math.pi/2))<math.radians(15))/len(segs)
    aspect=math.log(max(w/h,0.1) if h else 10, 2.718281828) / 2.3
    aspect=max(-1,min(1,aspect))
    turns_abs = [abs(d) for d in turns]
    concentration = sum(sorted(turns_abs, reverse=True)[:4]) / max(sum(turns_abs), 1e-9)
    return np.array([aspect, closed, 1-min(maxdev/length,1), corners, turn/2,
                     fill, rv/2, reversals/max(len(turns),1), min(length/diag,5)/5, aligned,
                     concentration], dtype=np.float64)

def noise(points, amount):
    return [(x+random.gauss(0,amount), y+random.gauss(0,amount)) for x,y in points]

def line():
    a=random.uniform(0,2*math.pi); L=random.uniform(80,400)
    x0,y0=random.uniform(-100,100),random.uniform(-100,100)
    x1,y1=x0+math.cos(a)*L,y0+math.sin(a)*L
    n=random.randint(16,64)
    bow=random.choice([0.0,0.0,0.01,0.02,0.03])*L*random.uniform(-1,1)
    nx,ny=-math.sin(a),math.cos(a)
    p=[(x0+(x1-x0)*i/(n-1)+nx*bow*math.sin(math.pi*i/(n-1)),
        y0+(y1-y0)*i/(n-1)+ny*bow*math.sin(math.pi*i/(n-1))) for i in range(n)]
    return noise(p, L*random.uniform(0.002,0.012))

def ellipse():
    cx,cy=random.uniform(-100,100),random.uniform(-100,100)
    rx=random.uniform(40,200); ry=rx*random.uniform(0.4,1.0)
    rot=random.uniform(0,math.pi); n=random.randint(32,80)
    span=2*math.pi if random.random()<0.8 else random.uniform(1.85*math.pi,2*math.pi)
    a0=random.uniform(0,2*math.pi)
    p=[]
    for i in range(n):
        t=a0+span*i/(n-1)
        x=rx*math.cos(t); y=ry*math.sin(t)
        p.append((cx+x*math.cos(rot)-y*math.sin(rot), cy+x*math.sin(rot)+y*math.cos(rot)))
    return noise(p,max(rx,ry)*random.uniform(0.002,0.008))

def rectangle():
    cx,cy=random.uniform(-100,100),random.uniform(-100,100)
    w=random.uniform(60,360); h=w*random.uniform(0.35,1.0)
    rot=random.uniform(0,math.pi/2); n=random.randint(32,80)
    rounded=random.choice([0.0,0.0,0.05,0.1,0.15])
    e=2/(1+rounded*6)
    p=[]
    for i in range(n):
        t=2*math.pi*i/(n-1)
        x=w/2*math.cos(t); y=h/2*math.sin(t)
        x=(abs(x)**e)*(1 if math.cos(t)>=0 else -1)
        y=(abs(y)**e)*(1 if math.sin(t)>=0 else -1)
        p.append((cx+x*math.cos(rot)-y*math.sin(rot), cy+x*math.sin(rot)+y*math.cos(rot)))
    return noise(p,max(w,h)*random.uniform(0.001,0.006))

def free():
    kind=random.randrange(5)
    n=random.randint(24,80)
    if kind==0:  # smooth spline
        knots=[(random.uniform(-150,150),random.uniform(-150,150)) for _ in range(random.randint(3,6))]
        p=[]
        for i in range(n):
            t=i*(len(knots)-1)/(n-1); k=min(int(t),len(knots)-2); u=t-k
            a,b,c,d=knots[max(k-1,0)],knots[k],knots[k+1],knots[min(k+2,len(knots)-1)]
            p.append((0.5*((2*b[0])+(-a[0]+c[0])*u+(2*a[0]-5*b[0]+4*c[0]-d[0])*u*u+(-a[0]+3*b[0]-3*c[0]+d[0])*u**3),
                      0.5*((2*b[1])+(-a[1]+c[1])*u+(2*a[1]-5*b[1]+4*c[1]-d[1])*u*u+(-a[1]+3*b[1]-3*c[1]+d[1])*u**3)))
        return noise(p,1.2)
    if kind==1:  # zigzag
        p=[]; x=random.uniform(-100,100); y=random.uniform(-100,100)
        for i in range(n):
            x+=random.uniform(2,8); y+=random.choice([-1,1])*random.uniform(8,24)
            p.append((x,y))
        return p
    if kind==2:  # spiral
        cx,cy=random.uniform(-50,50),random.uniform(-50,50)
        return [(cx+(8+i*1.4)*math.cos(i*0.35), cy+(8+i*1.4)*math.sin(i*0.35)) for i in range(n)]
    if kind==4:
        return line()
    # arc (should not become a full ellipse)
    cx,cy=random.uniform(-50,50),random.uniform(-50,50); r=random.uniform(60,200)
    a0=random.uniform(0,2*math.pi); span=random.uniform(0.8,4.6)
    return noise([(cx+r*math.cos(a0+span*i/(n-1)), cy+r*math.sin(a0+span*i/(n-1))) for i in range(n)],r*0.004)

X=[]; y=[]
for label,fn in [(0,line),(1,ellipse),(2,rectangle),(3,free)]:
    for _ in range(2500):
        f=features(fn())
        if f is not None:
            X.append(f); y.append(label)
X=np.array(X); y=np.array(y)
Xtr,Xte,ytr,yte=train_test_split(X,y,test_size=0.2,random_state=3,stratify=y)
clf=MLPClassifier(hidden_layer_sizes=(16,), activation='tanh', solver='adam',
                  learning_rate_init=0.01, max_iter=1600, random_state=3)
clf.fit(Xtr,ytr)
print('accuracy', accuracy_score(yte, clf.predict(Xte)))
print('coef0', clf.coefs_[0].shape, 'coef1', clf.coefs_[1].shape)
def fmt(a):
    return ', '.join(f'{v:.6f}' for v in a.ravel())
print('HIDDEN_W', fmt(clf.coefs_[0]))
print('HIDDEN_B', fmt(clf.intercepts_[0]))
print('OUT_W', fmt(clf.coefs_[1]))
print('OUT_B', fmt(clf.intercepts_[1]))
from pathlib import Path
out = Path(__file__).resolve().parents[1] / 'target' / 'shape_weights.npz'
out.parent.mkdir(parents=True, exist_ok=True)
np.savez(out, w0=clf.coefs_[0], b0=clf.intercepts_[0], w1=clf.coefs_[1], b1=clf.intercepts_[1])
print('weights', out)
