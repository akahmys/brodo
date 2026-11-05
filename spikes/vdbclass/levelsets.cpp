// Throwaway writer for spikes/vdbclass/ (ROADMAP.md P9, D5): level sets made by OpenVDB's
// own tools, through its public API, written with the file's default compression, so that
// brodo's reader can be held to what they hold. Built by hand against OpenVDB 14.0.0,
// never by brodo's build or gate.
//
//   levelsets FILE   writes every case to FILE, and prints each grid's class, value type,
//                    background, voxel size and the least and greatest value of its leaves,
//                    every voxel active or not, and of its tiles, as OpenVDB reads them
//   levelsets rotations FILE N   writes N level sets of one voxel under rotated transforms

#include <openvdb/io/File.h>
#include <openvdb/tools/Composite.h>
#include <openvdb/tools/LevelSetFilter.h>
#include <openvdb/tools/LevelSetPlatonic.h>
#include <openvdb/tools/LevelSetRebuild.h>
#include <openvdb/tools/LevelSetSphere.h>
#include <openvdb/tools/MeshToVolume.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <limits>
#include <string>
#include <vector>

namespace {

using openvdb::Vec3I;
using openvdb::Vec3s;

// A UV sphere of radius `r`, `n` rings and 2n segments, about 4n² triangles, slivers at
// its poles.
void uv_sphere(int n, double r, std::vector<Vec3s>& p, std::vector<Vec3I>& t) {
    const double pi = 3.14159265358979323846;
    p.clear();
    t.clear();
    p.emplace_back(0.f, 0.f, float(r));
    for (int i = 1; i < n; ++i) {
        double th = pi * i / n;
        for (int j = 0; j < 2 * n; ++j) {
            double ph = pi * j / n;
            p.emplace_back(float(r * std::sin(th) * std::cos(ph)),
                           float(r * std::sin(th) * std::sin(ph)), float(r * std::cos(th)));
        }
    }
    p.emplace_back(0.f, 0.f, float(-r));
    const unsigned s = 2 * n, last = unsigned(p.size()) - 1;
    auto at = [&](int ring, unsigned j) { return 1 + unsigned(ring - 1) * s + (j % s); };
    for (unsigned j = 0; j < s; ++j) t.emplace_back(0, at(1, j + 1), at(1, j));
    for (int i = 1; i + 1 < n; ++i)
        for (unsigned j = 0; j < s; ++j) {
            t.emplace_back(at(i, j), at(i, j + 1), at(i + 1, j + 1));
            t.emplace_back(at(i, j), at(i + 1, j + 1), at(i + 1, j));
        }
    for (unsigned j = 0; j < s; ++j) t.emplace_back(last, at(n - 1, j), at(n - 1, j + 1));
}

// A linear transform scaling by `s` along each axis, then rotating about x by `angle`
// radians, about y by 0.4·`angle` and about z by 1.3·`angle`, then translating by `d`.
openvdb::math::Transform::Ptr rotated(openvdb::Vec3d s, double angle, openvdb::Vec3d d) {
    openvdb::Mat4d m = openvdb::Mat4d::identity();
    m.preScale(s);
    m.postRotate(openvdb::math::X_AXIS, angle);
    m.postRotate(openvdb::math::Y_AXIS, 0.4 * angle);
    m.postRotate(openvdb::math::Z_AXIS, 1.3 * angle);
    m.postTranslate(d);
    return openvdb::math::Transform::createLinearTransform(m);
}

// A UV sphere's level set under `xf`, of half width 3 voxels each side.
openvdb::FloatGrid::Ptr mesh(const char* name, int n, double r,
                             openvdb::math::Transform::Ptr xf) {
    std::vector<Vec3s> world, index;
    std::vector<Vec3I> tris;
    uv_sphere(n, r, world, tris);
    for (const Vec3s& w : world) index.push_back(Vec3s(xf->worldToIndex(openvdb::Vec3d(w))));
    openvdb::tools::QuadAndTriangleDataAdapter<Vec3s, Vec3I> m(index, tris);
    auto g = openvdb::tools::meshToVolume<openvdb::FloatGrid>(m, *xf, 3.f, 3.f);
    g->setName(name);
    return g;
}

template <typename G>
void report(const G& g) {
    double bg = double(g.background());
    double inf = std::numeric_limits<double>::infinity();
    double lmin = inf, lmax = -inf, tmin = inf, tmax = -inf;
    size_t leaves = 0, over = 0, tiles = 0;
    for (auto l = g.tree().cbeginLeaf(); l; ++l) {
        ++leaves;
        for (auto v = l->cbeginValueAll(); v; ++v) {
            double x = double(*v);
            lmin = std::min(lmin, x);
            lmax = std::max(lmax, x);
            if (!(std::abs(x) <= bg)) ++over;
        }
    }
    for (auto v = g.tree().cbeginValueAll(); v; ++v) {
        if (v.getDepth() == g.tree().treeDepth() - 1) continue;
        ++tiles;
        tmin = std::min(tmin, double(*v));
        tmax = std::max(tmax, double(*v));
    }
    openvdb::Vec3d vs = g.voxelSize();
    std::printf("%s\t%s\t%s\tbg=%.9g\tvoxel=%.17g,%.17g,%.17g\tleaves=%zu\tleaf=[%.9g,%.9g]"
                "\tover=%zu\ttiles=%zu\ttile=[%.9g,%.9g]\n",
                g.getName().c_str(), g.valueType().c_str(),
                openvdb::GridBase::gridClassToString(g.getGridClass()).c_str(), bg, vs.x(),
                vs.y(), vs.z(), leaves, lmin, lmax, over, tiles, tmin, tmax);
}

}  // namespace

// `n` float level sets of one voxel, value 0 and background 3h, each under a linear
// transform of voxel size h, rotated about x, y and z by pseudo-random angles and
// translated, h drawn from five sizes.
openvdb::GridPtrVec rotations(int n) {
    openvdb::GridPtrVec out;
    std::uint64_t s = 0x9E3779B97F4A7C15ull;
    auto rnd = [&]() {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        return double(s >> 11) / double(1ull << 53) * 2.0 - 1.0;
    };
    const double sizes[] = {1.0, 0.1, 0.05, 1.0 / 3.0, 0.0123};
    for (int i = 0; i < n; ++i) {
        double h = sizes[i % 5];
        openvdb::Mat4d m = openvdb::Mat4d::identity();
        m.preScale(openvdb::Vec3d(h));
        m.postRotate(openvdb::math::X_AXIS, 3.14159265358979 * rnd());
        m.postRotate(openvdb::math::Y_AXIS, 3.14159265358979 * rnd());
        m.postRotate(openvdb::math::Z_AXIS, 3.14159265358979 * rnd());
        m.postTranslate(openvdb::Vec3d(10 * rnd(), 10 * rnd(), 10 * rnd()));
        auto g = openvdb::FloatGrid::create(float(3 * h));
        g->setTransform(openvdb::math::Transform::createLinearTransform(m));
        g->setGridClass(openvdb::GRID_LEVEL_SET);
        g->tree().setValue(openvdb::Coord(0), 0.f);
        char name[16];
        std::snprintf(name, sizeof name, "r%04d", i);
        g->setName(name);
        out.push_back(g);
    }
    return out;
}

int main(int argc, char** argv) {
    if (argc == 4 && std::string(argv[1]) == "rotations") {
        openvdb::initialize();
        openvdb::io::File(argv[2]).write(rotations(std::atoi(argv[3])));
        return 0;
    }
    if (argc != 2) {
        std::fprintf(stderr, "usage: levelsets FILE | levelsets rotations FILE N\n");
        return 2;
    }
    openvdb::initialize();
    using openvdb::FloatGrid;
    using openvdb::Vec3f;
    namespace t = openvdb::tools;
    openvdb::GridPtrVec out;
    auto add = [&](openvdb::GridBase::Ptr g, const char* name) {
        g->setName(name);
        out.push_back(g);
    };

    add(t::createLevelSetSphere<FloatGrid>(5.f, Vec3f(1.3f, -0.7f, 2.1f), 0.1f), "sphere");
    add(t::createLevelSetSphere<FloatGrid>(100.f, Vec3f(0.f), 1.f), "sphere_large");
    add(t::createLevelSetSphere<FloatGrid>(5.f, Vec3f(0.f), 0.1f, 2.5f), "sphere_halfwidth_2_5");
    add(t::createLevelSetSphere<openvdb::DoubleGrid>(5.f, Vec3f(0.f), 0.1f), "sphere_double");
    add(t::createLevelSetCube<FloatGrid>(4.f, Vec3f(0.3f), 0.1f), "cube");
    add(t::createLevelSetIcosahedron<FloatGrid>(4.f, Vec3f(0.f), 0.1f), "icosahedron");
    {
        auto a = t::createLevelSetSphere<FloatGrid>(3.f, Vec3f(0.f), 0.1f);
        auto b = t::createLevelSetSphere<FloatGrid>(3.f, Vec3f(2.f, 0.5f, 0.f), 0.1f);
        auto c = t::createLevelSetSphere<FloatGrid>(1.5f, Vec3f(1.f, 0.f, 2.5f), 0.1f);
        t::csgUnion(*a, *b);
        t::csgDifference(*a, *c);
        add(a, "csg");
    }
    {
        auto g = t::createLevelSetCube<FloatGrid>(4.f, Vec3f(0.f), 0.1f);
        t::LevelSetFilter<FloatGrid> f(*g);
        for (int i = 0; i < 5; ++i) f.meanCurvature();
        add(g, "filter_mean_curvature");
    }
    {
        auto g = t::createLevelSetCube<FloatGrid>(4.f, Vec3f(0.f), 0.1f);
        t::LevelSetFilter<FloatGrid> f(*g);
        f.gaussian(2);
        f.offset(0.25f);
        add(g, "filter_gaussian_offset");
    }
    {
        auto g = t::createLevelSetSphere<FloatGrid>(5.f, Vec3f(0.f), 0.1f);
        add(t::levelSetRebuild(*g, 0.1f, 3.f), "rebuild");
    }
    out.push_back(mesh("mesh", 160, 4.0, openvdb::math::Transform::createLinearTransform(0.05)));
    {
        auto xf = openvdb::math::Transform::createLinearTransform(0.05);
        xf->postTranslate(openvdb::Vec3d(-1.234, 0.5, 7.25));
        out.push_back(mesh("mesh_translated", 40, 4.0, xf));
    }
    out.push_back(mesh("mesh_rotated", 40, 4.0,
                       rotated(openvdb::Vec3d(0.05), 0.7, openvdb::Vec3d(0.3, -0.2, 1.1))));
    out.push_back(mesh("mesh_rotated_third", 40, 4.0,
                       rotated(openvdb::Vec3d(1.0 / 30.0), 2.1, openvdb::Vec3d(0.0))));
    out.push_back(mesh("mesh_nonuniform", 40, 4.0,
                       rotated(openvdb::Vec3d(0.05, 0.05, 0.1), 0.0, openvdb::Vec3d(0.0))));

    std::vector<Vec3s> p;
    std::vector<Vec3I> tr;
    uv_sphere(160, 1.0, p, tr);
    std::printf("# mesh: %zu triangles\n", tr.size());
    for (auto& g : out) {
        if (auto f = openvdb::gridPtrCast<FloatGrid>(g)) report(*f);
        else if (auto d = openvdb::gridPtrCast<openvdb::DoubleGrid>(g)) report(*d);
    }
    openvdb::io::File(argv[1]).write(out);
    return 0;
}
