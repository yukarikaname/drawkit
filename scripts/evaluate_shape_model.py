# Compare the committed shape model with traditional geometry and classical classifiers.
#
#   uv run --with numpy --with scikit-learn python scripts/evaluate_shape_model.py
#
# The "hard" set is deliberately outside the training distribution: bowed lines, rounded
# rectangles, partial ellipses labelled free, and noisy freehand. The number that matters for
# Auto Shape is precision at the confidence threshold, not raw accuracy: a wrong replacement is
# worse than leaving the stroke alone.
import math, random
import numpy as np
exec(open('/tmp/eval_shape.py').read().split('X,y=make')[0])
from sklearn.neural_network import MLPClassifier
from sklearn.linear_model import LogisticRegression
from sklearn.tree import DecisionTreeClassifier
from sklearn.neighbors import KNeighborsClassifier
from sklearn.ensemble import HistGradientBoostingClassifier
from sklearn.metrics import accuracy_score, f1_score, confusion_matrix

def gens_aug():
    return [
        lambda: line(bow=random.choice([0.0,0.0,0.01,0.02,0.03])),
        lambda: ellipse(span=2*math.pi) if random.random()<0.8 else ellipse(span=random.uniform(1.85*math.pi,2*math.pi)),
        lambda: rectangle(rounded=random.choice([0.0,0.0,0.05,0.1,0.15])),
        lambda: free(),
    ]
def gens_hard():
    return [lambda:line(bow=random.choice([0.02,0.04,0.06])),
            lambda:ellipse(span=2*math.pi) if random.random()<0.6 else ellipse(span=random.uniform(1.2*math.pi,1.9*math.pi)),
            lambda:rectangle(rounded=random.choice([0.0,0.05,0.1,0.15])), free]

def dataset(gens,count):
    X=[];y=[]
    for label in range(4):
        for _ in range(count):
            f=features(gens[label]())
            if f is not None:X.append(f);y.append(label)
    return np.array(X),np.array(y)

Xa,ya=dataset(gens_aug(),3000)
Xh,yh=dataset(gens_hard(),500)
models={
 'mlp':MLPClassifier(hidden_layer_sizes=(16,),activation='tanh',solver='adam',learning_rate_init=0.01,max_iter=2000,random_state=3),
 'logreg':LogisticRegression(max_iter=5000),
 'tree4':DecisionTreeClassifier(max_depth=4,random_state=3),
 'tree6':DecisionTreeClassifier(max_depth=6,random_state=3),
 'knn5':KNeighborsClassifier(n_neighbors=5),
 'gb':HistGradientBoostingClassifier(max_iter=200,random_state=3),
}
for name,m in models.items():
    m.fit(Xa,ya)
    for tag,XX,yy in [('aug',Xa,ya),('hard',Xh,yh)]:
        p=m.predict(XX)
        print(name,tag,'acc',round(accuracy_score(yy,p),4),'f1',round(f1_score(yy,p,average='macro'),4))
    print(' cm',confusion_matrix(yh,m.predict(Xh)).tolist())


def rdp(points, eps):
    if len(points) < 3:
        return points
    a, b = points[0], points[-1]
    dx, dy = b[0] - a[0], b[1] - a[1]
    L = math.hypot(dx, dy) or 1
    best, index = -1.0, 0
    for i in range(1, len(points) - 1):
        d = abs((points[i][0] - a[0]) * dy - (points[i][1] - a[1]) * dx) / L
        if d > best:
            best, index = d, i
    if best > eps:
        return rdp(points[:index + 1], eps)[:-1] + rdp(points[index:], eps)
    return [a, b]

def traditional(points):
    q = resample(points, 32)
    if len(q) < 4:
        return 3
    xs = [p[0] for p in q]; ys = [p[1] for p in q]
    w = max(xs) - min(xs); h = max(ys) - min(ys)
    length = sum(math.dist(q[i - 1], q[i]) for i in range(1, len(q))) or 1
    closed = 1 - math.dist(q[0], q[-1]) / length
    ax, ay = q[0]; bx, by = q[-1]
    L = math.hypot(bx - ax, by - ay) or 1
    line_rms = (sum(abs((x - ax) * (by - ay) - (y - ay) * (bx - ax)) / L for x, y in q)
                / len(q)) / max(L, 1e-6)
    if line_rms < 0.06 and closed < 0.35:
        return 0
    simp = rdp(q, length * 0.035)
    corners = len(simp)
    area = abs(sum(simp[i - 1][0] * simp[i][1] - simp[i][0] * simp[i - 1][1]
                   for i in range(1, len(simp))) / 2)
    fill = area / max(w * h, 1e-6)
    if closed > 0.5 and simp[0] != simp[-1]:
        area = abs(sum(simp[i - 1][0] * simp[i][1] - simp[i][0] * simp[i - 1][1]
                       for i in range(1, len(simp)))
                   + simp[-1][0] * simp[0][1] - simp[0][0] * simp[-1][1]) / 2
        fill = area / max(w * h, 1e-6)
    if closed > 0.62 and corners >= 6 and fill < 0.86:
        return 1
    if closed > 0.62 and corners <= 6 and fill > 0.86:
        return 2
    return 3

def deployed_predict(X):
    weights = np.load('target/shape_weights.npz')
    hidden = np.tanh(X @ weights['w0'] + weights['b0'])
    logits = hidden @ weights['w1'] + weights['b1']
    logits -= logits.max(axis=1, keepdims=True)
    prob = np.exp(logits)
    return prob / prob.sum(axis=1, keepdims=True)

# Traditional geometry on the hard set.
Pt = []
Yt = []
for label in range(4):
    for _ in range(500):
        points = gens_hard()[label]()
        Pt.append(traditional(points))
        Yt.append(label)
Pt = np.array(Pt); Yt = np.array(Yt)
print('traditional hard acc', round(accuracy_score(Yt, Pt), 4),
      'f1', round(f1_score(Yt, Pt, average='macro'), 4))
print(' cm', confusion_matrix(Yt, Pt).tolist())

# The committed weights, evaluated on the same hard features.
prob = deployed_predict(Xh)
pred = prob.argmax(axis=1); conf = prob.max(axis=1)
print('deployed mlp hard acc', round(accuracy_score(yh, pred), 4),
      'f1', round(f1_score(yh, pred, average='macro'), 4))
for threshold in [0.7, 0.8, 0.9]:
    accepted = (conf >= threshold) & (pred != 3)
    if accepted.sum():
        correct = (pred[accepted] == yh[accepted]).sum()
        print(' threshold', threshold, 'accepted', int(accepted.sum()), 'of', len(yh),
              'precision', round(correct / accepted.sum(), 4))

# Confidence/coverage trade-off of the trained MLP.

mlp=models['mlp']
prob=mlp.predict_proba(Xh)
pred=prob.argmax(axis=1); conf=prob.max(axis=1)
for threshold in [0.5,0.7,0.8,0.9,0.95]:
    accepted=(conf>=threshold)&(pred!=3)
    if accepted.sum()==0:continue
    correct=(pred[accepted]==yh[accepted]).sum()
    print('threshold',threshold,'accepted',int(accepted.sum()),'of',len(yh),
          'precision',round(correct/accepted.sum(),4))
