from django.db import transaction
from django.core.exceptions import ValidationError
from apps.reservations.models import Reservation

class ReservationService:
    @staticmethod
    @transaction.atomic
    def create(**data):
        overlap=Reservation.objects.filter(hotel_id=data['hotel_id'],room=data['room'],status='CONFIRMED',check_in_date__lt=data['check_out_date'],check_out_date__gt=data['check_in_date']).exists()
        if overlap: raise ValidationError('Reservation overlap detected')
        return Reservation.objects.create(**data)
