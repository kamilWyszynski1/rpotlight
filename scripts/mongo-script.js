db = connect('mongodb://rpotlight:rpotlight@localhost:27017/?authMechanism=DEFAULT');

use discoverer
db.registered.deleteMany({})

use registry
db.parsed.deleteMany({})
db.content.deleteMany({})
