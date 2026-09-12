"""DesktopFly art prototype: original procedural natural-history habitat meshes.
Authored for Blender 5.1; no downloaded/generated image assets or context operators.
"""
import bpy
import math
import random
from mathutils import Vector
from mathutils.noise import noise


def build_habitat(scene, collection):
    rng = random.Random(4207)
    objects = []

    def mesh(name, verts, faces, mats, indices=None, smooth=True):
        data = bpy.data.meshes.new(name)
        data.from_pydata(verts, [], faces)
        data.update()
        ob = bpy.data.objects.new(name, data)
        collection.objects.link(ob)
        for mat in mats:
            data.materials.append(mat)
        for i, poly in enumerate(data.polygons):
            poly.use_smooth = smooth
            if indices:
                poly.material_index = indices[i]
        objects.append(ob)
        return ob

    def material(name, colors, scale=8, bump=.12, roughness=.85):
        mat = bpy.data.materials.new(name)
        mat.diffuse_color = (*colors[0], 1)
        mat.use_nodes = True
        n = mat.node_tree.nodes
        l = mat.node_tree.links
        p = n.get('Principled BSDF')
        p.inputs['Roughness'].default_value = roughness
        coords = n.new('ShaderNodeTexCoord')
        tex = n.new('ShaderNodeTexNoise')
        tex.inputs['Scale'].default_value = scale
        tex.inputs['Detail'].default_value = 5
        tex.inputs['Roughness'].default_value = .72
        l.new(coords.outputs['Object'], tex.inputs['Vector'])
        ramp = n.new('ShaderNodeValToRGB')
        ramp.color_ramp.elements[0].position = .18
        ramp.color_ramp.elements[0].color = (*colors[0], 1)
        ramp.color_ramp.elements[1].position = .8
        ramp.color_ramp.elements[1].color = (*colors[1], 1)
        l.new(tex.outputs['Fac'], ramp.inputs[0])
        l.new(ramp.outputs[0], p.inputs['Base Color'])
        fine = n.new('ShaderNodeTexNoise')
        fine.inputs['Scale'].default_value = scale * 14
        fine.inputs['Detail'].default_value = 3
        l.new(coords.outputs['Object'], fine.inputs['Vector'])
        bn = n.new('ShaderNodeBump')
        bn.inputs['Strength'].default_value = .48
        bn.inputs['Distance'].default_value = bump
        l.new(fine.outputs['Fac'], bn.inputs['Height'])
        l.new(bn.outputs['Normal'], p.inputs['Normal'])
        return mat

    sand = material('Warm washed sand • mineral variation', ((.39,.30,.20),(.47,.37,.25)), 28, .005)
    stone = material('Weathered buff sandstone', ((.19,.16,.115),(.43,.365,.255)), 4, .055)
    cork = material('Cork • ash brown crevices', ((.023,.014,.009),(.10,.061,.032)), 11, .018)
    corkedge = material('Cork • worn exposed ridges', ((.055,.032,.018),(.17,.10,.050)), 13, .012)
    leafmats = [material('Oak litter %d'%i, (a,b), 8, .008) for i,(a,b) in enumerate([
        ((.10,.042,.014),(.29,.13,.035)), ((.20,.105,.035),(.43,.245,.085)),
        ((.13,.065,.027),(.32,.195,.075))])]
    veinmat = material('Dry pale leaf ribs', ((.23,.14,.053),(.43,.28,.10)), 10, .002)
    green = material('Muted succulent waxy leaves', ((.07,.12,.065),(.23,.29,.125)), 5, .006, .59)

    # Radial mesh with a quiet central resting surface and gently broken edge.
    verts = [(0,0,-.025)]
    nr, nt = 40, 180
    for j in range(1,nr+1):
        r = 5.2*j/nr
        for i in range(nt):
            a=math.tau*i/nt
            edge=1 + .014*math.sin(a*7)+.009*math.sin(a*13+1)
            x,y=r*math.cos(a)*edge,r*math.sin(a)*.91*edge
            amp=max(0,min(1,(r-2.8)/1.5))
            z=-.025+amp*(.045*math.sin(x*2.2)*math.cos(y*2.8)+.022*math.sin(x*6+y*4))
            verts.append((x,y,z))
    faces=[(0,1+i,1+(i+1)%nt) for i in range(nt)]
    for j in range(nr-1):
        for i in range(nt):
            a=1+j*nt+i; b=1+j*nt+(i+1)%nt
            faces.append((a,a+nt,b+nt,b))
    topstart=1+(nr-1)*nt
    for i in range(nt):
        x,y,z=verts[topstart+i]; verts.append((x,y,-.20))
    lowstart=len(verts)-nt
    for i in range(nt):
        faces.append((topstart+i,lowstart+i,lowstart+(i+1)%nt,topstart+(i+1)%nt))
    mesh('Sandy terrarium floor',verts,faces,[sand])

    # Small actual angular grains complement the shader; consolidated into one mesh.
    gv=[]; gf=[]
    grainmats=[material('Sand grain %d'%i,(c,tuple(v*1.15 for v in c)),20,.002) for i,c in enumerate([
        (.38,.29,.18),(.53,.43,.29),(.23,.19,.12),(.63,.54,.40)])]
    gi=[]
    for k in range(1050):
        a=rng.uniform(0,math.tau); r=math.sqrt(rng.random())*5
        x,y=r*math.cos(a),r*math.sin(a)*.9
        # Keep grains unobtrusive beneath the hero body.
        s=rng.uniform(.008,.027); z=-.018
        base=len(gv)
        for dx,dy,dz in [(s,0,0),(0,s*.8,0),(-s,0,0),(0,-s*.8,0),(0,0,s*.9)]:
            gv.append((x+dx,y+dy,z+dz))
        mid=rng.randrange(4)
        for q in range(4): gf.append((base+q,base+(q+1)%4,base+4)); gi.append(mid)
    mesh('Scattered mineral grains',gv,gf,grainmats,gi,False)

    def pebble(name, center, size, seed):
        rr=random.Random(seed); vv=[]; ff=[]; ns=26; rings=12
        phases=[rr.random()*6 for _ in range(3)]
        for j in range(rings+1):
            p=math.pi*j/rings
            for i in range(ns):
                a=math.tau*i/ns
                wobble=1+.10*math.sin(a*3+phases[0])+.065*math.sin(a*5+p*4+phases[1])
                vv.append((center[0]+size[0]*math.sin(p)*math.cos(a)*wobble,
                           center[1]+size[1]*math.sin(p)*math.sin(a)*wobble,
                           center[2]+size[2]*math.cos(p)*(1+.10*math.sin(a*4+phases[2]))))
        for j in range(rings):
            for i in range(ns):
                a=j*ns+i; b=j*ns+(i+1)%ns; ff.append((a,b,b+ns,a+ns))
        return mesh(name,vv,ff,[stone])
    for i,(pos,size) in enumerate([
        ((3.8,1.8,.13),(.52,.39,.24)),((4.05,1.05,.07),(.29,.23,.14)),
        ((-3.9,-.6,.10),(.44,.31,.19)),((-3.6,-1.2,.045),(.22,.16,.11)),
        ((2.9,-3.0,.06),(.28,.22,.13)),((2.5,3.3,.13),(.51,.34,.25))]):
        pebble('Worn sandstone %02d'%i,pos,size,920+i)

    # Hollow half cork tube; aperture faces the viewer, with genuine furrow relief.
    vv=[]; ff=[]; mi=[]; nlong=90; narc=96
    for layer in range(2):
        for j in range(nlong+1):
            t=j/nlong
            y=2.45+t*1.9
            for i in range(narc+1):
                a=math.pi*i/narc
                # Variable winding paths and broken plates remove the uniform
                # extrusion rhythm; isotropic cells fracture longitudinal grain.
                warp=noise(Vector((a*2.7,t*3.6,5.21)))
                ridge=math.sin(a*29+warp*4.5+t*.7)
                broken=noise(Vector((a*15.1,t*9.3,1.6)))
                groove=.039*ridge+.067*broken+.034*noise(Vector((a*5,t*5,9)))
                groove-=.048*max(0,noise(Vector((a*10,t*16,7)))-.08)
                rad=(1.27 if layer==0 else 1.07)+(groove if layer==0 else groove*.28)
                edgewave=.064*noise(Vector((a*9,0,3)))+.024*math.sin(a*39)
                vv.append((-.1+math.cos(a)*rad,y+edgewave, .015+math.sin(a)*rad*.77))
    layerstride=(nlong+1)*(narc+1)
    for layer in range(2):
        for j in range(nlong):
            for i in range(narc):
                a=layer*layerstride+j*(narc+1)+i
                ff.append((a,a+1,a+narc+2,a+narc+1))
                patch=noise(Vector((i*.18,j*.19,2.1)))
                mi.append(1 if layer==0 and patch>.17 else 0)
    for j in [0,nlong]:
        for i in range(narc):
            a=j*(narc+1)+i
            ff.append((a,a+layerstride,a+layerstride+1,a+1)); mi.append(1)
    mesh('Hollow weathered cork shelter',vv,ff,[cork,corkedge],mi)

    def tube(name, points, radius, mat):
        vv=[]; ff=[]; sides=5
        for j,p in enumerate(points):
            t=Vector(points[min(j+1,len(points)-1)])-Vector(points[max(j-1,0)])
            t.normalize(); u=t.cross(Vector((0,0,1)))
            if u.length<.01: u=t.cross(Vector((0,1,0)))
            u.normalize(); v=t.cross(u)
            for i in range(sides):
                q=Vector(p)+radius*(u*math.cos(math.tau*i/sides)+v*math.sin(math.tau*i/sides))
                vv.append(q)
        for j in range(len(points)-1):
            for i in range(sides):
                a=j*sides+i; b=j*sides+(i+1)%sides; ff.append((a,b,b+sides,a+sides))
        return mesh(name,vv,ff,[mat])

    # Oak silhouettes have uneven rounded lobes, a curved midrib and lifted edges.
    for k,(x,y,length,angle) in enumerate([(-3.3,-2.1,1.1,.4),
        (3.8,-1.5,.95,1.2),(-2.45,3.5,.8,2.5),
        (1.8,-3.75,.75,2.7),(-4.15,1.0,.8,1.8),(3.5,-2.65,.66,.5)]):
        curl=rng.uniform(.04,.19); twist=rng.uniform(-.09,.09)
        phase=rng.uniform(-.25,.25); skew=rng.uniform(-.2,.2)
        r=math.sqrt(x*x+(y/.91)**2)
        ground=-.025+max(0,min(1,(r-2.8)/1.5))*(.045*math.sin(x*2.2)*math.cos(y*2.8)+.022*math.sin(x*6+y*4))
        def leafpoint(t,w):
            z=ground+.018+curl*(t-.3)**2+.055*math.sin(t*math.pi)*w*w+twist*w*math.sin(math.pi*t)
            xx=length*(t-.5); yy=length*(w*.29+.035*math.sin(t*math.pi))
            return (x+xx*math.cos(angle)-yy*math.sin(angle),y+xx*math.sin(angle)+yy*math.cos(angle),z)
        vv=[]; ff=[]; count=84
        for j in range(count+1):
            t=j/count
            envelope=math.sin(math.pi*t)**.65
            # Rounded broad lobes alternate imperfectly across the midrib.
            left=envelope*(.68+.19*math.cos(t*math.pi*(8+k%3)+phase))*(1+skew)
            right=envelope*(.68+.20*math.cos(t*math.pi*(8+k%3)+.8+phase))*(1-skew)
            for w in [-left,0,right]: vv.append(leafpoint(t,w))
        for j in range(count):
            for side in range(2):
                a=j*3+side; ff.append((a,a+3,a+4,a+1))
        mesh('Curled fallen oak leaf %02d'%k,vv,ff,[leafmats[k%3]],smooth=True)
        tube('Oak leaf midrib %02d'%k,[tuple(v+(.008 if q==2 else 0) for q,v in enumerate(leafpoint(j/16,0))) for j in range(17)],.006,veinmat)
        for t in [.2,.36,.52,.68,.8]:
            for sign in [-1,1]:
                tube('Oak secondary rib', [leafpoint(t,0),leafpoint(t+.055,sign*.35),leafpoint(min(.96,t+.1),sign*.6)],.0025,veinmat)

    # Restrained fleshy rosette, each lanceolate leaf modeled with a rounded ridge.
    for k in range(19):
        ring=0 if k<11 else 1
        a=math.tau*(k if not ring else k-11)/(11 if not ring else 8)+ring*.35
        length=rng.uniform(.61,.9) if not ring else rng.uniform(.44,.62)
        vv=[]; ff=[]; rows=18; cols=8
        for j in range(rows+1):
            t=j/rows; w=math.sin(math.pi*t)**.75*.16*(1 if not ring else .85)
            radius=.08+t*length*(.93 if not ring else .63)
            for i in range(cols+1):
                u=(i/cols)*2-1
                z=.04+t*(.25 if not ring else .62)+.07*(1-u*u)*math.sin(math.pi*t)
                vv.append((-3.35+radius*math.cos(a)-u*w*math.sin(a),2.22+radius*math.sin(a)+u*w*math.cos(a),z))
        for j in range(rows):
            for i in range(cols):
                q=j*(cols+1)+i; ff.append((q,q+1,q+cols+2,q+cols+1))
        ob=mesh('Succulent fleshy lance %02d'%k,vv,ff,[green])
        mod=ob.modifiers.new('Leaf edge flesh','SOLIDIFY'); mod.thickness=.018
    return {'objects': objects}
