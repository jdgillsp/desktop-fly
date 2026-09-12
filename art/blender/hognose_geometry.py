"""Western hognose study geometry. Author: Codex visual-production agent.

Data API only; the caller owns the scene, materials, lighting, and rendering.
"""
import math
import bpy
from mathutils import Vector


def build_hognose(scene, collection, materials):
    objects = []

    def mesh(name, verts, faces, mat, uvs=None):
        data = bpy.data.meshes.new(name)
        data.from_pydata(verts, [], faces)
        data.update()
        ob = bpy.data.objects.new(name, data)
        collection.objects.link(ob)
        objects.append(ob)
        if mat is not None:
            data.materials.append(mat)
        for polygon in data.polygons:
            polygon.use_smooth = True
        if uvs is not None:
            layer = data.uv_layers.new(name='SkinUV')
            for polygon in data.polygons:
                for li, vi in zip(polygon.loop_indices, polygon.vertices):
                    layer.data[li].uv = uvs[vi]
        return ob

    def line(name, pts, radius, mat):
        data = bpy.data.curves.new(name, 'CURVE')
        data.dimensions = '3D'
        data.resolution_u = 12
        data.bevel_depth = radius
        data.bevel_resolution = 3
        spline = data.splines.new('BEZIER')
        spline.bezier_points.add(len(pts)-1)
        for bp, co in zip(spline.bezier_points, pts):
            bp.co = co
            bp.handle_left_type = 'AUTO'
            bp.handle_right_type = 'AUTO'
        ob = bpy.data.objects.new(name, data)
        collection.objects.link(ob)
        data.materials.append(mat)
        objects.append(ob)
        return ob

    def ellipsoid(name, center, radii, mat, orientation=None, rings=20, sectors=40):
        verts, faces = [], []
        c = Vector(center)
        for i in range(rings+1):
            lat = math.pi*i/rings
            for j in range(sectors):
                lon = 2*math.pi*j/sectors
                p = Vector((radii[0]*math.sin(lat)*math.cos(lon),
                            radii[1]*math.sin(lat)*math.sin(lon),
                            radii[2]*math.cos(lat)))
                if orientation is not None:
                    p = orientation @ p
                verts.append(c+p)
        for i in range(rings):
            for j in range(sectors):
                k=i*sectors+j
                q=i*sectors+(j+1)%sectors
                faces.append((k,k+sectors,q+sectors,q))
        return mesh(name,verts,faces,mat)

    # Head-to-tail path: the outer loop and returning inner loop are offset,
    # leaving broad resting contact instead of a stack of intersecting toruses.
    controls = [
        (0,-3.00,.77),(0,-2.82,.65),(0,-2.52,.54),(0,-2.23,.50),
        (0,-1.96,.45),(-.02,-1.72,.43),(-.20,-1.39,.42),
        (-.88,-1.24,.42),(-1.68,-.93,.43),(-2.18,-.12,.44),
        (-2.15,.94,.44),(-1.60,1.77,.44),(-.63,2.14,.44),
        (.50,2.12,.44),(1.49,1.66,.44),(2.08,.78,.44),
        (2.12,-.25,.44),(1.73,-1.18,.44),(1.03,-1.72,.45),
        (.57,-1.37,.46),(.69,-.60,.48),(1.01,.15,.48),
        (.70,.89,.47),(-.05,1.18,.46),(-.86,.81,.43),
        (-1.12,.10,.40),(-.80,-.51,.35),(-.27,-.60,.28),
        (.02,-.24,.22),(.24,.03,.16),(.34,.37,.10),(.20,.62,.085)
    ]
    cp=[Vector(v) for v in controls]
    raw=[]
    for k in range(len(cp)-1):
        a=cp[max(0,k-1)]; b=cp[k]; c=cp[k+1]; d=cp[min(len(cp)-1,k+2)]
        count=max(8,int((c-b).length*30))
        for n in range(count):
            t=n/count
            raw.append(.5*((2*b)+(-a+c)*t+(2*a-5*b+4*c-d)*t*t+(-a+3*b-3*c+d)*t*t*t))
    raw.append(cp[-1])
    distance=[0.0]
    for a,b in zip(raw,raw[1:]): distance.append(distance[-1]+(b-a).length)
    total=distance[-1]

    # Snout is a low, broad rostral shovel, with no conical protrusion.
    profile=[(0,.125,.036),(.07,.20,.075),(.18,.255,.13),(.34,.335,.215),
             (.57,.435,.29),(.80,.48,.30),(1.03,.405,.27),(1.27,.30,.26),
             (1.65,.315,.30),(2.3,.37,.345)]
    def radii(s):
        if s<profile[-1][0]:
            for a,b in zip(profile,profile[1:]):
                if s<=b[0]:
                    t=(s-a[0])/(b[0]-a[0]); t=t*t*(3-2*t)
                    return a[1]+(b[1]-a[1])*t,a[2]+(b[2]-a[2])*t
        remain=total-s
        if remain<2.8:
            f=max(.012,(remain/2.8)**.85)
            return .35*f,.32*f
        w=.365+.024*math.sin(s*.38)
        return w,w*.91

    verts=[]; faces=[]; uvs=[]; sectors=64
    for i,(p,s) in enumerate(zip(raw,distance)):
        tangent=(raw[min(i+1,len(raw)-1)]-raw[max(i-1,0)]).normalized()
        side=Vector((tangent.y,-tangent.x,0)).normalized()
        up=side.cross(tangent).normalized()
        width,height=radii(s)
        for j in range(sectors+1):
            v=j/sectors; ang=2*math.pi*v
            # Slightly flatter ventrum gives the animal believable ground contact.
            z=math.sin(ang)
            if z<-.65: z=-.65+(z+.65)*.66
            pt=p+side*(width*math.cos(ang))+up*(height*z)
            verts.append(tuple(pt)); uvs.append((s/total,v))
    for i in range(len(raw)-1):
        for j in range(sectors):
            k=i*(sectors+1)+j
            faces.append((k,k+sectors+1,k+sectors+2,k+1))
    faces.extend([tuple(range(sectors)),tuple((len(raw)-1)*(sectors+1)+j for j in reversed(range(sectors)))])
    animal=mesh('Hognose | continuous anatomical skin',verts,faces,materials['body'],uvs)
    animal.data.materials.append(materials['head'])
    animal.data.materials.append(materials['belly'])
    for i,poly in enumerate(animal.data.polygons):
        ring=i//sectors; sector=i%sectors
        if ring<len(distance):
            if distance[ring]<1.2: poly.material_index=1
            if math.sin(2*math.pi*(sector+.5)/sectors)<-.62: poly.material_index=2
    animal.data.polygons[-2].material_index=1

    # Eyes are lateral, partly seated in the cheek; the exposed dark lens remains
    # smaller than the surrounding natural head shields.
    for sign,label in [(-1,'L'),(1,'R')]:
        normal=Vector((sign*.87,-.30,.25)).normalized()
        rotation=Vector((0,0,1)).rotation_difference(normal).to_matrix()
        center=Vector((sign*.401,-2.365,.655))
        ellipsoid('Eye '+label+' | seated cornea',center,(.098,.098,.024),materials['eye'],rotation)
        ellipsoid('Iris '+label+' | warm brown ring',center+normal*.019,(.068,.068,.007),materials['iris'],rotation)
        ellipsoid('Pupil '+label+' | round',center+normal*.027,(.053,.053,.004),materials['eye'],rotation)
        # A small supraocular brow seats the lens without an inflated eye socket.
        nostril=Vector((sign*.244,-2.733,.665))
        n=Vector((sign*.75,-.40,.50)).normalized()
        rot=Vector((0,0,1)).rotation_difference(n).to_matrix()
        ellipsoid('Nostril '+label,nostril,(.027,.018,.007),materials['crease'],rot,rings=12,sectors=24)
        # The continuous skin supplies the ventral lip; no coplanar overlays.

    for ob in objects:
        ob.location.z-=.08

    return {'objects':objects,'focus':(0,0,.40),'head_focus':(0,-2.37,.48),
            'body':animal,'length':total,'path':[tuple(p) for p in raw],
            'distances':distance,'sectors':sectors}
