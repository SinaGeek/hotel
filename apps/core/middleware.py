from threading import local
_storage=local()

def get_current_hotel_id():
    return getattr(_storage,'hotel_id',None)

class HotelContextMiddleware:
    def __init__(self,get_response): self.get_response=get_response
    def __call__(self,request):
        header=request.headers.get('X-Hotel-ID')
        _storage.hotel_id=int(header) if header and header.isdigit() else None
        if request.user.is_authenticated and not _storage.hotel_id:
            _storage.hotel_id=getattr(request.user,'hotel_id',None)
        request.hotel_id=_storage.hotel_id
        return self.get_response(request)
