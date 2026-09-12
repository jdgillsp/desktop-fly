"""Non-destructive Blender to DesktopFly deformable hognose asset export."""
import json
from pathlib import Path
import bpy
from mathutils import Vector


def export_hognose(snake, collection, outpath):
    body = snake['body']
    path = [body.matrix_world @ Vector(p) for p in snake['path']]
    distances = snake['distances']
    length = float(snake['length'])
    vertices, indices, parts = [], [], []
    cache = {}

    def frame(i):
        tangent = (path[min(i+1,len(path)-1)]-path[max(0,i-1)]).normalized()
        side = Vector((tangent.y,-tangent.x,0)).normalized()
        return tangent, side, side.cross(tangent).normalized()

    frames = [frame(i) for i in range(len(path))]

    def material_values(mat):
        name = mat.name.lower() if mat else ''
        color = list(mat.diffuse_color) if mat else [0.55,0.43,0.28,1.0]
        # Blender material values are linear; runtime vertex colours are display-encoded.
        color = [(12.92*c if c <= .0031308 else 1.055*c**(1/2.4)-.055) for c in color[:3]] + [color[3]]
        parameters = [.58,.16,.12,.08]
        weight = 0.0
        if 'scales' in name or 'head' in name:
            color = [1.,1.,1.,1.]
            weight = 1.0
        elif 'eye' in name:
            parameters = [.16,.62,0.,.08]
        elif 'iris' in name:
            parameters = [.24,.40,0.,.06]
        elif 'crease' in name:
            parameters = [.74,.08,0.,.02]
        return color,parameters,weight

    def emit(key, p, normal, uv, mat, i, center=None, s=None):
        if key in cache:
            return cache[key]
        tangent,side,up = frames[i]
        center = path[i] if center is None else center
        s = distances[i] if s is None else s
        offset = p-center
        color,parameters,weight = material_values(mat)
        data = [s/length,offset.dot(tangent),offset.dot(side),
                offset.dot(up)+center.z-.36,
                normal.dot(tangent),normal.dot(side),normal.dot(up),
                float(uv[0]),float(uv[1]),*color,*parameters,weight]
        index = len(vertices)
        vertices.append([round(float(v),7) for v in data])
        cache[key] = index
        return index

    sectors = snake['sectors']
    stride = sectors+1
    rings = [i for i in range(len(path)) if i%3==0 or (distances[i]<1.5 and i%2==0)]
    if rings[-1] != len(path)-1:
        rings.append(len(path)-1)
    angles = list(range(0,sectors+1))
    normal_matrix = body.matrix_world.to_3x3().inverted().transposed()
    def body_vertex(i,j,material_index):
        v = body.data.vertices[i*stride+j]
        return emit(('body',i,j,material_index),body.matrix_world@v.co,
                    (normal_matrix@v.normal).normalized(),(distances[i]/length,j/sectors),
                    body.data.materials[material_index],i)
    for a,b in zip(rings,rings[1:]):
        for c,d in zip(angles,angles[1:]):
            material_index=body.data.polygons[a*sectors+c].material_index
            q=[body_vertex(i,j,material_index) for i,j in [(a,c),(b,c),(b,d),(a,d)]]
            indices.extend([q[0],q[1],q[2],q[0],q[2],q[3]])
    for ring,reverse in [(rings[0],False),(rings[-1],True)]:
        normal = -frames[ring][0] if ring == 0 else frames[ring][0]
        q=[]
        for j in angles[:-1]:
            vertex = body.data.vertices[ring*stride+j]
            q.append(emit(('cap',ring,j),body.matrix_world@vertex.co,normal,
                          (0.,.25),body.data.materials[2],ring))
        if reverse:q.reverse()
        for j in range(1,len(q)-1):indices.extend([q[0],q[j],q[j+1]])
    parts.append({'name':'continuous skin','first_index':0,'index_count':len(indices)})

    # Facial details attach to a projected point on the first part of the spine.
    # Segment projection prevents coarse nearest-ring binding from causing steps.
    def nearest(p):
        best=None
        for i in range(len(path)-1):
            if distances[i]>1.5:break
            segment=path[i+1]-path[i]
            f=max(0.,min(1.,(p-path[i]).dot(segment)/max(segment.length_squared,1e-12)))
            center=path[i]+f*segment
            distance=(p-center).length_squared
            if best is None or distance<best[0]:
                best=(distance,i,center,distances[i]+f*(distances[i+1]-distances[i]))
        return best[1:]

    depsgraph=bpy.context.evaluated_depsgraph_get()
    for obj in snake['objects']:
        if obj==body or obj.type not in {'MESH','CURVE'}:continue
        evaluated=obj.evaluated_get(depsgraph)
        mesh=evaluated.to_mesh()
        try:
            mesh.calc_loop_triangles()
            transform=evaluated.matrix_world
            normals=transform.to_3x3().inverted().transposed()
            uv_layer=mesh.uv_layers.active
            start=len(indices)
            for triangle in mesh.loop_triangles:
                mat=mesh.materials[triangle.material_index] if mesh.materials else None
                for vi,li in zip(triangle.vertices,triangle.loops):
                    vertex=mesh.vertices[vi]
                    p=transform@vertex.co
                    i,center,s=nearest(p)
                    uv=uv_layer.data[li].uv if uv_layer else (s/length,.25)
                    normal=(normals@vertex.normal).normalized()
                    indices.append(emit((obj.name,vi,triangle.material_index,tuple(uv)),p,normal,uv,mat,i,center,s))
            parts.append({'name':obj.name,'first_index':start,'index_count':len(indices)-start})
        finally:
            evaluated.to_mesh_clear()
    if len(vertices)>25000 or len(indices)//3>45000:
        raise ValueError('Hognose asset exceeds runtime budget: %d vertices, %d triangles'%(len(vertices),len(indices)//3))
    payload={'version':1,'reference_length':length,'vertices':vertices,'indices':indices,'parts':parts}
    target=Path(outpath)
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(json.dumps(payload,separators=(',',':')),encoding='utf-8')
    return {'path':str(target),'vertices':len(vertices),'triangles':len(indices)//3,'parts':len(parts),'reference_length':length}
