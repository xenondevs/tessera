package xyz.xenondevs.tessera.capture.dump

import org.joml.Vector2fc
import org.joml.Vector3f
import org.joml.Vector3fc
import kotlin.math.abs

enum class QuadShape {
    
    PARALLELOGRAM,
    GENERAL,
    INVISIBLE;
    
    companion object {
        
        fun of(positions: List<Vector3fc>, uvs: List<Vector2fc>): QuadShape {
            require(positions.size == 4 && uvs.size == 4) { "Expected 4 vertices, got ${positions.size} positions and ${uvs.size} uvs" }
            
            val (p0, p1, p2, p3) = positions
            if (Vector3f(p2).sub(p0).cross(Vector3f(p3).sub(p1)).lengthSquared() < 1e-12f)
                return INVISIBLE
            
            val (uv0, uv1, uv2, uv3) = uvs
            
            if (abs(p0.x() + p2.x() - p1.x() - p3.x()) < 1e-5f
                && abs(p0.y() + p2.y() - p1.y() - p3.y()) < 1e-5f
                && abs(p0.z() + p2.z() - p1.z() - p3.z()) < 1e-5f
                && abs(uv0.x() + uv2.x() - uv1.x() - uv3.x()) < 1e-3f
                && abs(uv0.y() + uv2.y() - uv1.y() - uv3.y()) < 1e-3f
            ) return PARALLELOGRAM
            
            return GENERAL
        }
        
    }
    
}
