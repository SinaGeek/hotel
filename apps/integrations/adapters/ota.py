class BaseIntegrationAdapter:
    def pull_reservations(self): return []
class BookingAdapter(BaseIntegrationAdapter):
    def pull_reservations(self): return [{'provider':'BOOKING','external_ref':'BKG-1'}]
class ExpediaAdapter(BaseIntegrationAdapter):
    def pull_reservations(self): return [{'provider':'EXPEDIA','external_ref':'EXP-1'}]
