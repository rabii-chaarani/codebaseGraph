#include <string>

namespace app {
class User {
public:
    std::string name();
};

std::string User::name() {
    return std::string();
}

void helper() {
    User user;
    user.name();
}
}
