// Throwaway checker for spikes/vdbvec/ (P12's issue 3): OpenVDB's own library, through its
// public API, as a writer and reader of vector grids apart from brodo's. Built by hand
// against OpenVDB 12.0.0, 13.0.0 and 14.0.0, never by brodo's build or gate.
//
//   oracle info           the library and file versions, and the compression a new file has
//   oracle types          whether each grid type a name is tried for is registered, and the
//                         type names of trees of uint8_t and uint32_t
//   oracle write FILE [half-]default|zip|none|mask, or half
//                         three grids (`grids` below), with that compression; `half-` (and
//                         `half`, the default) with the float vectors saved as half floats
//   oracle dump FILE      every grid's name, type, class, vector type, background,
//                         transform, then each leaf's origin and its 512 values with their
//                         active bits, and each tile with its value and active bit, sorted;
//                         a vector's components joined by commas
//
// The OpenVDB headers, the library's main header and its io/File.h, are included by the
// owner by hand (ROADMAP.md §4, on scripts/hooks/provenance_guard.py), before the line below.

#include <algorithm>
#include <openvdb/openvdb.h>
#include <openvdb/io/File.h>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

namespace {

template <typename T> struct Components { static constexpr int n = 1; };
template <typename T> struct Components<openvdb::math::Vec3<T>> { static constexpr int n = 3; };

double component(float v, int) { return v; }
double component(double v, int) { return v; }
template <typename T> double component(const openvdb::math::Vec3<T>& v, int k) { return double(v[k]); }

template <typename V> std::string text(const V& v)
{
    std::string out;
    char part[40];
    for (int k = 0; k < Components<V>::n; ++k) {
        std::snprintf(part, sizeof part, k ? ",%.17g" : "%.17g", component(v, k));
        out += part;
    }
    return out;
}

// A vector of three distinct components from one number, so that a component swapped or
// dropped shows.
template <typename V> V of(double x)
{
    using T = typename V::value_type;
    return V(T(x), T(-2 * x + 0.25), T(x * x / 8 - 1));
}

// Seven leaves, each of whose inactive voxels hold one of the patterns by which OpenVDB
// writes a node's inactive values: the background alone, its negation alone, one other
// value, the background and its negation, the background and another, two others, and
// three others; each leaf with a few active voxels of their own values. Then an active
// lower, upper and root tile and an inactive lower tile of another value.
template <typename GridT> typename GridT::Ptr patterned(const std::string& name, double scale)
{
    using V = typename GridT::ValueType;
    const V bg = of<V>(1.5), neg = -bg, a = of<V>(7), b = of<V>(-3), c = of<V>(11);
    const std::vector<std::vector<V>> inactive = {
        {bg}, {neg}, {a}, {bg, neg}, {bg, a}, {a, b}, {a, b, c}};
    typename GridT::Ptr grid = GridT::create(bg);
    for (size_t leaf = 0; leaf < inactive.size(); ++leaf) {
        auto* node = grid->tree().touchLeaf(openvdb::Coord(8 * int(leaf), 8, -8));
        const auto& others = inactive[leaf];
        for (openvdb::Index i = 0; i < 512; ++i) node->setValueOff(i, others[i % others.size()]);
        for (openvdb::Index i = 0; i < 512; i += 41) node->setValueOn(i, of<V>(0.5 * i - 3 + leaf));
    }
    grid->tree().addTile(1, openvdb::Coord(64, 0, 0), of<V>(2.5), true);
    grid->tree().addTile(1, openvdb::Coord(72, 0, 0), of<V>(-6), false);
    grid->tree().addTile(2, openvdb::Coord(128, 0, 0), of<V>(4), true);
    grid->tree().addTile(3, openvdb::Coord(0, 4096, 0), of<V>(9), true);
    grid->setName(name);
    grid->setTransform(openvdb::math::Transform::createLinearTransform(scale));
    return grid;
}

// A float vector grid of covariant vectors, a double vector grid of no vector type set, and
// an empty float vector grid.
openvdb::GridPtrVec grids(bool half)
{
    openvdb::Vec3SGrid::Ptr s = patterned<openvdb::Vec3SGrid>("v3s", 0.5);
    s->setVectorType(openvdb::VEC_COVARIANT);
    s->setSaveFloatAsHalf(half);
    openvdb::Vec3DGrid::Ptr d = patterned<openvdb::Vec3DGrid>("v3d", 0.25);
    openvdb::Vec3SGrid::Ptr empty = openvdb::Vec3SGrid::create();
    empty->setName("v3s_empty");
    return openvdb::GridPtrVec{s, d, empty};
}

int write(const std::string& path, std::string compression)
{
    const bool half = compression.rfind("half", 0) == 0;
    if (half) compression = compression == "half" ? "default" : compression.substr(5);
    openvdb::io::File file(path);
    if (compression == "zip") file.setCompression(openvdb::io::COMPRESS_ZIP | openvdb::io::COMPRESS_ACTIVE_MASK);
    else if (compression == "none") file.setCompression(openvdb::io::COMPRESS_NONE);
    else if (compression == "mask") file.setCompression(openvdb::io::COMPRESS_ACTIVE_MASK);
    else if (compression != "default") {
        std::fprintf(stderr, "compression: [half-]default, zip, none or mask, or half\n");
        return 2;
    }
    file.write(grids(half));
    file.close();
    return 0;
}

int info()
{
    openvdb::io::File file("unused.vdb");
    std::printf("library\t%s\tfile_version\t%u\tabi\t%d\n", OPENVDB_LIBRARY_VERSION_STRING,
        unsigned(openvdb::OPENVDB_FILE_VERSION), OPENVDB_ABI_VERSION_NUMBER);
    std::printf("default_compression\t%u\t%s\n", file.compression(),
        openvdb::io::compressionToString(file.compression()).c_str());
    return 0;
}

int types()
{
    const char* names[] = {"Tree_bool_5_4_3", "Tree_float_5_4_3", "Tree_double_5_4_3",
        "Tree_int32_5_4_3", "Tree_int64_5_4_3", "Tree_uint8_5_4_3", "Tree_uint32_5_4_3",
        "Tree_int8_5_4_3", "Tree_uint16_5_4_3", "Tree_vec3i_5_4_3", "Tree_vec3s_5_4_3",
        "Tree_vec3d_5_4_3", "Tree_mask_5_4_3"};
    for (const char* n : names) std::printf("registered\t%s\t%d\n", n, int(openvdb::GridBase::isRegistered(n)));
    std::printf("tree_of\tuint8_t\t%s\n", openvdb::tree::Tree4<uint8_t, 5, 4, 3>::Type::treeType().c_str());
    std::printf("tree_of\tuint32_t\t%s\n", openvdb::tree::Tree4<uint32_t, 5, 4, 3>::Type::treeType().c_str());
    std::printf("tree_of\tVec3s\t%s\n", openvdb::Vec3STree::treeType().c_str());
    std::printf("tree_of\tVec3d\t%s\n", openvdb::Vec3DTree::treeType().c_str());
    return 0;
}

template <typename GridT> void dumpTree(const GridT& grid)
{
    std::printf("background\t%s\n", text(grid.background()).c_str());
    std::vector<std::string> rows;
    char line[96];
    for (auto it = grid.tree().cbeginLeaf(); it; ++it) {
        const openvdb::Coord o = it->origin();
        std::snprintf(line, sizeof line, "leaf\t%d\t%d\t%d", o.x(), o.y(), o.z());
        std::string row = line;
        for (openvdb::Index i = 0; i < GridT::TreeType::LeafNodeType::SIZE; ++i)
            row += "\t" + text(it->getValue(i)) + ":" + std::to_string(int(it->isValueOn(i)));
        rows.emplace_back(row + "\n");
    }
    for (auto it = grid.tree().cbeginValueAll(); it; ++it) {
        if (it.isVoxelValue()) continue;
        const openvdb::CoordBBox box = it.getBoundingBox();
        std::snprintf(line, sizeof line, "tile\t%d\t%d\t%d\t%d\t", box.min().x(), box.min().y(),
            box.min().z(), box.dim().x());
        rows.emplace_back(line + text(*it) + ":" + std::to_string(int(it.isValueOn())) + "\n");
    }
    std::sort(rows.begin(), rows.end());
    for (const auto& r : rows) std::fputs(r.c_str(), stdout);
}

int dump(const std::string& path)
{
    openvdb::io::File file(path);
    file.open();
    std::printf("file\tversion\t%u\n", file.fileVersion());
    for (auto name = file.beginName(); name != file.endName(); ++name) {
        openvdb::GridBase::Ptr base = file.readGrid(name.gridName());
        std::printf("grid\t%s\ttype\t%s\tclass\t%s\tvector_type\t%s\thalf\t%d\tvoxels\t%llu\n",
            base->getName().c_str(), base->type().c_str(),
            openvdb::GridBase::gridClassToString(base->getGridClass()).c_str(),
            openvdb::GridBase::vecTypeToString(base->getVectorType()).c_str(), int(base->saveFloatAsHalf()),
            static_cast<unsigned long long>(base->activeVoxelCount()));
        for (auto m = base->beginMeta(); m != base->endMeta(); ++m)
            std::printf("meta\t%s\t%s\t%s\n", m->first.c_str(), m->second->typeName().c_str(), m->second->str().c_str());
        const openvdb::math::Mat4d m = base->transform().baseMap()->getAffineMap()->getMat4();
        std::printf("map\t%s", base->transform().mapType().c_str());
        for (int r = 0; r < 4; ++r) for (int c = 0; c < 4; ++c) std::printf("\t%.17g", m(r, c));
        std::printf("\n");
        if (auto g = openvdb::gridPtrCast<openvdb::FloatGrid>(base)) dumpTree(*g);
        else if (auto g = openvdb::gridPtrCast<openvdb::DoubleGrid>(base)) dumpTree(*g);
        else if (auto g = openvdb::gridPtrCast<openvdb::Vec3SGrid>(base)) dumpTree(*g);
        else if (auto g = openvdb::gridPtrCast<openvdb::Vec3DGrid>(base)) dumpTree(*g);
        else std::printf("unhandled\n");
    }
    file.close();
    return 0;
}

} // namespace

int main(int argc, char** argv)
{
    openvdb::initialize();
    const std::string mode = argc > 1 ? argv[1] : "";
    try {
        if (mode == "info" && argc == 2) return info();
        if (mode == "types" && argc == 2) return types();
        if (mode == "write" && argc == 4) return write(argv[2], argv[3]);
        if (mode == "dump" && argc == 3) return dump(argv[2]);
    } catch (const std::exception& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
    std::fprintf(stderr, "usage: oracle info | oracle types | oracle write FILE default|zip|none|mask|half | oracle dump FILE\n");
    return 2;
}
