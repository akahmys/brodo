// Throwaway checker for spikes/vdb/ (P7, ADR-0045): OpenVDB's own library, through its
// public API, as a reader apart from brodo's. Built by hand against the owner's install of
// OpenVDB 14.0.0, never by brodo's build or gate.
//
//   oracle dump FILE      every grid's name, type, class, background, transform, active
//                         voxel count, then each active voxel and tile, sorted
//   oracle sphere FILE C  a float level set of a sphere, written by OpenVDB with
//                         compression C (none, zip, blosc), for comparing bytes
//   oracle rewrite IN OUT C   IN read and written again by OpenVDB with compression C
//                         (none, mask), for comparing its bytes with brodo's

#include <openvdb/openvdb.h>
#include <openvdb/io/File.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

namespace {

template <typename GridT>
void dumpValues(const GridT& grid)
{
    std::vector<std::string> rows;
    for (auto it = grid.tree().cbeginValueOn(); it; ++it) {
        const openvdb::CoordBBox box = it.getBoundingBox();
        char line[256];
        std::snprintf(line, sizeof line, "%d\t%d\t%d\t%d\t%d\t%d\t%.17g\n",
            box.min().x(), box.min().y(), box.min().z(),
            box.max().x(), box.max().y(), box.max().z(),
            static_cast<double>(*it));
        rows.emplace_back(line);
    }
    std::sort(rows.begin(), rows.end());
    for (const auto& r : rows) std::fputs(r.c_str(), stdout);
}

int dump(const std::string& path)
{
    openvdb::io::File file(path);
    file.open();
    for (auto name = file.beginName(); name != file.endName(); ++name) {
        openvdb::GridBase::Ptr base = file.readGrid(name.gridName());
        std::printf("grid\t%s\ttype\t%s\tclass\t%s\tvoxels\t%llu\n", base->getName().c_str(),
            base->type().c_str(), openvdb::GridBase::gridClassToString(base->getGridClass()).c_str(),
            static_cast<unsigned long long>(base->activeVoxelCount()));
        const openvdb::math::Mat4d m = base->transform().baseMap()->getAffineMap()->getMat4();
        std::printf("map\t%s", base->transform().mapType().c_str());
        for (int r = 0; r < 4; ++r) for (int c = 0; c < 4; ++c) std::printf("\t%.17g", m(r, c));
        std::printf("\n");
        if (auto g = openvdb::gridPtrCast<openvdb::FloatGrid>(base)) {
            std::printf("background\t%.17g\n", static_cast<double>(g->background()));
            dumpValues(*g);
        } else if (auto g = openvdb::gridPtrCast<openvdb::DoubleGrid>(base)) {
            std::printf("background\t%.17g\n", g->background());
            dumpValues(*g);
        } else {
            std::printf("unhandled\n");
        }
    }
    file.close();
    return 0;
}

int sphere(const std::string& path, const std::string& compression)
{
    // The textbook |p| - r clamped to a band of 3, radius 20, as a level set.
    const float r = 20.0f, band = 3.0f;
    openvdb::FloatGrid::Ptr grid = openvdb::FloatGrid::create(band);
    auto acc = grid->getAccessor();
    for (int z = -24; z < 24; ++z) for (int y = -24; y < 24; ++y) for (int x = -24; x < 24; ++x) {
        const float d = std::sqrt(float(x * x + y * y + z * z)) - r;
        if (std::fabs(d) < band) acc.setValue(openvdb::Coord(x, y, z), d);
    }
    grid->setName("sdf");
    grid->setGridClass(openvdb::GRID_LEVEL_SET);
    openvdb::io::File file(path);
    if (compression == "none") file.setCompression(openvdb::io::COMPRESS_NONE);
    else if (compression == "zip") file.setCompression(openvdb::io::COMPRESS_ZIP | openvdb::io::COMPRESS_ACTIVE_MASK);
    else if (compression == "blosc") file.setCompression(openvdb::io::COMPRESS_BLOSC | openvdb::io::COMPRESS_ACTIVE_MASK);
    else { std::fprintf(stderr, "compression: none, zip or blosc\n"); return 2; }
    file.write(openvdb::GridPtrVec{grid});
    file.close();
    return 0;
}

int rewrite(const std::string& in, const std::string& out, const std::string& compression)
{
    openvdb::io::File src(in);
    src.open();
    openvdb::GridPtrVecPtr grids = src.getGrids();
    src.close();
    openvdb::io::File dst(out);
    if (compression == "none") dst.setCompression(openvdb::io::COMPRESS_NONE);
    else if (compression == "mask") dst.setCompression(openvdb::io::COMPRESS_ACTIVE_MASK);
    else { std::fprintf(stderr, "compression: none or mask\n"); return 2; }
    dst.write(*grids);
    dst.close();
    return 0;
}

} // namespace

int main(int argc, char** argv)
{
    openvdb::initialize();
    const std::string mode = argc > 1 ? argv[1] : "";
    try {
        if (mode == "dump" && argc == 3) return dump(argv[2]);
        if (mode == "sphere" && argc == 4) return sphere(argv[2], argv[3]);
        if (mode == "rewrite" && argc == 5) return rewrite(argv[2], argv[3], argv[4]);
    } catch (const std::exception& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
    std::fprintf(stderr, "usage: oracle dump FILE | oracle sphere FILE none|zip|blosc | oracle rewrite IN OUT none|mask\n");
    return 2;
}
